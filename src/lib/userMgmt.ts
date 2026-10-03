/**
 * Database user management (Task 10) — pure per-engine SQL generation, mirroring
 * `tableScript.ts`'s "Copy script as…": these build a string, they never execute. The
 * caller (`UsersView.tsx`) previews the SQL (password rendered as `'***'`) and runs the
 * real statement through `runScript`, which records no history.
 *
 * Supported engines only: postgres, mysql, sqlserver (`supportsUserMgmt`). MariaDB has no
 * separate `Engine` id in this build, so it isn't listed.
 */
import type { Engine, SessionCaps } from "./commands";
import { quoteIdent, quoteLiteral } from "./tableScript";

export type GrantPreset = "read" | "readwrite" | "all";

export interface UserSpec {
  name: string;
  password: string;
  /** MySQL's user@host clause; ignored elsewhere. Defaults to `%` (any host), same as
   *  MySQL's own default when a user is created without one. */
  host?: string;
}

export interface DropUserSpec {
  name: string;
  host?: string;
  /** SQL Server: the login's user in the current database (the listing's `database_user`).
   *  Null/absent falls back to the login name. */
  dbUser?: string | null;
}

const DEFAULT_HOST = "%";

function unsupported(engine: Engine): never {
  throw new Error(`Database user management is not supported for ${engine}`);
}

export function supportsUserMgmt(engine: Engine, caps?: SessionCaps): boolean {
  return (engine === "postgres" || engine === "mysql" || engine === "sqlserver") && (caps?.user_mgmt ?? true);
}

export function listUsersSql(engine: Engine): string {
  switch (engine) {
    case "postgres":
      return "SELECT rolname, rolsuper, rolcanlogin, rolcreatedb, rolvaliduntil FROM pg_roles WHERE rolname !~ '^pg_' ORDER BY 1";
    case "mysql":
      return "SELECT User, Host, account_locked FROM mysql.user ORDER BY 1,2";
    case "sqlserver":
      return (
        "SELECT sp.name, sp.type_desc, sp.is_disabled, dp.name AS database_user " +
        "FROM sys.server_principals sp " +
        "LEFT JOIN sys.database_principals dp ON dp.sid = sp.sid " +
        "WHERE sp.type IN ('S','U','G') AND sp.name NOT LIKE '##%' " +
        "ORDER BY sp.name"
      );
    default:
      return unsupported(engine);
  }
}

/** SQL Server literal: `N'…'` — doubles `'`, no backslash escaping (mirrors `quoteLiteral`'s
 *  standard/non-MySQL branch, just with the `N` prefix Unicode literals need). */
function nLiteral(value: string): string {
  return `N${quoteLiteral(value)}`;
}

export function createUserSql(engine: Engine, { name, password, host = DEFAULT_HOST }: UserSpec): string {
  switch (engine) {
    case "postgres":
      return `CREATE ROLE ${quoteIdent(name)} LOGIN PASSWORD ${quoteLiteral(password)};`;
    case "mysql":
      return `CREATE USER ${quoteLiteral(name, "mysql")}@${quoteLiteral(host, "mysql")} IDENTIFIED BY ${quoteLiteral(password, "mysql")};`;
    case "sqlserver": {
      const id = quoteIdent(name, "sqlserver");
      return `CREATE LOGIN ${id} WITH PASSWORD = ${nLiteral(password)}; CREATE USER ${id} FOR LOGIN ${id};`;
    }
    default:
      return unsupported(engine);
  }
}

export function dropUserSql(
  engine: Engine,
  { name, host = DEFAULT_HOST, dbUser }: DropUserSpec,
): string {
  switch (engine) {
    case "postgres":
      return `DROP ROLE ${quoteIdent(name)};`;
    case "mysql":
      return `DROP USER ${quoteLiteral(name, "mysql")}@${quoteLiteral(host, "mysql")};`;
    case "sqlserver": {
      // The user (in the current database) must go before the login it depends on. A login
      // can have no user here; an unguarded DROP USER would fail AFTER the batch went on to
      // drop the login, reporting failure for a drop that happened. Worse, DROP USER can
      // fail for an UNRELATED reason (it owns a schema) while the batch keeps going and drops
      // the login anyway — a half-succeeded drop with a misleading error. TRY/CATCH + THROW
      // stops the login from being touched unless the user step actually completed.
      const user = dbUser ?? name;
      return (
        `BEGIN TRY IF USER_ID(${nLiteral(user)}) IS NOT NULL DROP USER ${quoteIdent(user, "sqlserver")}; ` +
        `DROP LOGIN ${quoteIdent(name, "sqlserver")}; END TRY BEGIN CATCH THROW; END CATCH`
      );
    }
    default:
      return unsupported(engine);
  }
}

export function changePasswordSql(
  engine: Engine,
  { name, password, host = DEFAULT_HOST }: UserSpec,
): string {
  switch (engine) {
    case "postgres":
      return `ALTER ROLE ${quoteIdent(name)} PASSWORD ${quoteLiteral(password)};`;
    case "mysql":
      return `ALTER USER ${quoteLiteral(name, "mysql")}@${quoteLiteral(host, "mysql")} IDENTIFIED BY ${quoteLiteral(password, "mysql")};`;
    case "sqlserver":
      return `ALTER LOGIN ${quoteIdent(name, "sqlserver")} WITH PASSWORD = ${nLiteral(password)};`;
    default:
      return unsupported(engine);
  }
}

/** Table privileges per preset, Postgres spelling. */
const PG_PRIVS: Record<GrantPreset, string> = {
  read: "SELECT",
  readwrite: "SELECT, INSERT, UPDATE, DELETE",
  all: "ALL",
};
/** Table privileges per preset, MySQL spelling (`ALL` needs the `PRIVILEGES` keyword). */
const MYSQL_PRIVS: Record<GrantPreset, string> = {
  read: "SELECT",
  readwrite: "SELECT, INSERT, UPDATE, DELETE",
  all: "ALL PRIVILEGES",
};
/** SQL Server fixed database role(s) per preset. `readwrite` is deliberately BOTH roles —
 *  `db_datawriter` alone can write but not read, so "read/write" needs `db_datareader` too
 *  (unlike PG/MySQL, where a single privilege list already covers both). */
const MSSQL_ROLES: Record<GrantPreset, string[]> = {
  read: ["db_datareader"],
  readwrite: ["db_datareader", "db_datawriter"],
  all: ["db_owner"],
};

/** The principal a grant/revoke names. SQL Server's fixed database roles take the login's
 *  database user (the listing's `database_user`), so a login with no user in this database
 *  has nothing to grant to — `null`, for the caller to say so. Elsewhere, the listed name. */
export function grantTarget(engine: Engine, name: string, dbUser: string | null): string | null {
  return engine === "sqlserver" ? dbUser : name;
}

export function grantSql(
  engine: Engine,
  preset: GrantPreset,
  user: string,
  database: string,
  schema: string,
  host: string = DEFAULT_HOST,
): string {
  switch (engine) {
    case "postgres": {
      const u = quoteIdent(user);
      const sc = quoteIdent(schema);
      const stmts = [
        `GRANT CONNECT ON DATABASE ${quoteIdent(database)} TO ${u};`,
        `GRANT USAGE ON SCHEMA ${sc} TO ${u};`,
        `GRANT ${PG_PRIVS[preset]} ON ALL TABLES IN SCHEMA ${sc} TO ${u};`,
      ];
      // A serial/identity column's INSERT calls nextval() on its sequence — without USAGE
      // on it, the insert fails even though the table grant above succeeded. `read` never
      // inserts, so it needs no sequence privileges.
      if (preset === "readwrite" || preset === "all") {
        stmts.push(`GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA ${sc} TO ${u};`);
      }
      return stmts.join("\n");
    }
    case "mysql":
      // MySQL grants target `user@host` specifically — the caller must pass the SAME host
      // the row was listed under, or this silently changes privileges on a different account.
      return `GRANT ${MYSQL_PRIVS[preset]} ON ${quoteIdent(database, "mysql")}.* TO ${quoteLiteral(user, "mysql")}@${quoteLiteral(host, "mysql")};`;
    case "sqlserver":
      // Fixed database roles are scoped to the database the connection is already in —
      // `database`/`schema` don't appear in the statement itself.
      return MSSQL_ROLES[preset]
        .map((role) => `ALTER ROLE ${role} ADD MEMBER ${quoteIdent(user, "sqlserver")};`)
        .join("\n");
    default:
      return unsupported(engine);
  }
}

export function revokeSql(
  engine: Engine,
  preset: GrantPreset,
  user: string,
  database: string,
  schema: string,
  host: string = DEFAULT_HOST,
): string {
  switch (engine) {
    case "postgres": {
      const u = quoteIdent(user);
      const sc = quoteIdent(schema);
      const stmts: string[] = [];
      if (preset === "readwrite" || preset === "all") {
        stmts.push(`REVOKE USAGE, SELECT ON ALL SEQUENCES IN SCHEMA ${sc} FROM ${u};`);
      }
      stmts.push(
        `REVOKE ${PG_PRIVS[preset]} ON ALL TABLES IN SCHEMA ${sc} FROM ${u};`,
        `REVOKE USAGE ON SCHEMA ${sc} FROM ${u};`,
        `REVOKE CONNECT ON DATABASE ${quoteIdent(database)} FROM ${u};`,
      );
      return stmts.join("\n");
    }
    case "mysql":
      return `REVOKE ${MYSQL_PRIVS[preset]} ON ${quoteIdent(database, "mysql")}.* FROM ${quoteLiteral(user, "mysql")}@${quoteLiteral(host, "mysql")};`;
    case "sqlserver":
      return MSSQL_ROLES[preset]
        .map((role) => `ALTER ROLE ${role} DROP MEMBER ${quoteIdent(user, "sqlserver")};`)
        .join("\n");
    default:
      return unsupported(engine);
  }
}
