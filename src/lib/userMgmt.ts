/**
 * Database user management (Task 10) — pure per-engine SQL generation, mirroring
 * `tableScript.ts`'s "Copy script as…": these build a string, they never execute. The
 * caller (`UsersView.tsx`) previews the SQL (password rendered as `'***'`) and runs the
 * real statement through `runScript`, which records no history.
 *
 * Supported engines only: postgres, mysql, sqlserver (`supportsUserMgmt`). MariaDB has no
 * separate `Engine` id in this build, so it isn't listed.
 */
import type { Engine } from "./commands";
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
}

const DEFAULT_HOST = "%";

function unsupported(engine: Engine): never {
  throw new Error(`Database user management is not supported for ${engine}`);
}

export function supportsUserMgmt(engine: Engine): boolean {
  return engine === "postgres" || engine === "mysql" || engine === "sqlserver";
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

export function dropUserSql(engine: Engine, { name, host = DEFAULT_HOST }: DropUserSpec): string {
  switch (engine) {
    case "postgres":
      return `DROP ROLE ${quoteIdent(name)};`;
    case "mysql":
      return `DROP USER ${quoteLiteral(name, "mysql")}@${quoteLiteral(host, "mysql")};`;
    case "sqlserver": {
      const id = quoteIdent(name, "sqlserver");
      // The user (in the current database) must go before the login it depends on.
      return `DROP USER ${id}; DROP LOGIN ${id};`;
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
/** SQL Server fixed database role per preset. */
const MSSQL_ROLES: Record<GrantPreset, string> = {
  read: "db_datareader",
  readwrite: "db_datawriter",
  all: "db_owner",
};

export function grantSql(
  engine: Engine,
  preset: GrantPreset,
  user: string,
  database: string,
  schema: string,
): string {
  switch (engine) {
    case "postgres": {
      const u = quoteIdent(user);
      const sc = quoteIdent(schema);
      return [
        `GRANT CONNECT ON DATABASE ${quoteIdent(database)} TO ${u};`,
        `GRANT USAGE ON SCHEMA ${sc} TO ${u};`,
        `GRANT ${PG_PRIVS[preset]} ON ALL TABLES IN SCHEMA ${sc} TO ${u};`,
      ].join("\n");
    }
    case "mysql":
      // MySQL grants target `user@host`; there's no host parameter here, so this always
      // grants to the same `%` host `createUserSql` defaults to.
      return `GRANT ${MYSQL_PRIVS[preset]} ON ${quoteIdent(database, "mysql")}.* TO ${quoteLiteral(user, "mysql")}@${quoteLiteral(DEFAULT_HOST, "mysql")};`;
    case "sqlserver":
      // Fixed database roles are scoped to the database the connection is already in —
      // `database`/`schema` don't appear in the statement itself.
      return `ALTER ROLE ${MSSQL_ROLES[preset]} ADD MEMBER ${quoteIdent(user, "sqlserver")};`;
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
): string {
  switch (engine) {
    case "postgres": {
      const u = quoteIdent(user);
      const sc = quoteIdent(schema);
      return [
        `REVOKE ${PG_PRIVS[preset]} ON ALL TABLES IN SCHEMA ${sc} FROM ${u};`,
        `REVOKE USAGE ON SCHEMA ${sc} FROM ${u};`,
        `REVOKE CONNECT ON DATABASE ${quoteIdent(database)} FROM ${u};`,
      ].join("\n");
    }
    case "mysql":
      return `REVOKE ${MYSQL_PRIVS[preset]} ON ${quoteIdent(database, "mysql")}.* FROM ${quoteLiteral(user, "mysql")}@${quoteLiteral(DEFAULT_HOST, "mysql")};`;
    case "sqlserver":
      return `ALTER ROLE ${MSSQL_ROLES[preset]} DROP MEMBER ${quoteIdent(user, "sqlserver")};`;
    default:
      return unsupported(engine);
  }
}
