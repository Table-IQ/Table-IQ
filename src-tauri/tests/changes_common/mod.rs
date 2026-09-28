//! `apply_changes` scenarios every engine runs (tests/apply_changes.rs, tests/sqlserver.rs).
//! The fixture is a plain `tiq_changes (id INT PRIMARY KEY, name VARCHAR(20))` in the
//! connection's default schema; statements run one by one so every engine accepts them.
use arrow::array::{Array, StringArray};
use tableiq_lib::drivers::{Change, ChangesResult, EditSpec, SqlDriver};

pub const TABLE: &str = "tiq_changes";

pub async fn seed(drv: &dyn SqlDriver) {
    drv.run_script("DROP TABLE IF EXISTS tiq_changes").await.expect("drop");
    drv.run_script("CREATE TABLE tiq_changes (id INT PRIMARY KEY, name VARCHAR(20))").await.expect("create");
    drv.run_script("INSERT INTO tiq_changes (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c'), (4, 'd')")
        .await
        .expect("seed");
}

fn pk(id: &str) -> Vec<(String, Option<String>)> {
    vec![("id".into(), Some(id.into()))]
}
pub fn update(id: &str, from: &str, to: &str) -> Change {
    Change::Update {
        edit: EditSpec { pk: pk(id), column: "name".into(), new_value: Some(to.into()), orig_value: Some(from.into()) },
    }
}
pub fn delete(id: &str) -> Change {
    Change::Delete { pk: pk(id) }
}
pub fn insert(id: &str, name: Option<&str>) -> Change {
    Change::Insert { values: vec![("id".into(), Some(id.into())), ("name".into(), name.map(String::from))] }
}

/// `(id, name)` in id order, as the database has it now.
pub async fn rows(drv: &dyn SqlDriver) -> Vec<(String, Option<String>)> {
    let b = drv.run_query("SELECT id, name FROM tiq_changes ORDER BY id", 100).await.expect("read back").batch;
    let col = |i: usize| b.column(i).as_any().downcast_ref::<StringArray>().expect("Utf8").clone();
    let (ids, names) = (col(0), col(1));
    (0..b.num_rows())
        .map(|r| (ids.value(r).to_string(), (!names.is_null(r)).then(|| names.value(r).to_string())))
        .collect()
}

fn row(id: &str, name: Option<&str>) -> (String, Option<String>) {
    (id.into(), name.map(String::from))
}

/// 2 updates, 2 deletes, 1 insert, handed over insert-first: re-inserting id 3 only
/// succeeds if the backend really runs deletes before inserts.
pub async fn commits_all_in_order(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    let changes =
        vec![insert("3", Some("new")), delete("3"), update("1", "a", "A"), delete("4"), update("2", "b", "B")];
    let res = drv.apply_changes(schema, TABLE, &changes).await.expect("apply_changes");
    assert_eq!(res, ChangesResult { updated: 2, deleted: 2, inserted: 1 });
    assert_eq!(rows(drv).await, vec![row("1", Some("A")), row("2", Some("B")), row("3", Some("new"))]);
}

/// Writing the value a cell already holds still matches its row (MySQL reports matched,
/// not changed, rows because sqlx sets CLIENT_FOUND_ROWS) — it is not a conflict.
pub async fn same_value_update_is_not_a_conflict(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    let res = drv.apply_changes(schema, TABLE, &[update("1", "a", "a")]).await.expect("same value");
    assert_eq!(res, ChangesResult { updated: 1, deleted: 0, inserted: 0 });
}

pub async fn null_and_empty_inserts_stay_distinct(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    drv.apply_changes(schema, TABLE, &[insert("5", None), insert("6", Some(""))]).await.expect("inserts");
    let all = rows(drv).await;
    assert_eq!(&all[4..], &[row("5", None), row("6", Some(""))]);
}

/// Row 2 is renamed behind the app's back after the grid loaded it: its update matches 0
/// rows, and the updates, delete and insert around it must all roll back.
pub async fn conflict_rolls_back_everything(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    drv.run_script("UPDATE tiq_changes SET name = 'zz' WHERE id = 2").await.expect("concurrent write");
    let before = rows(drv).await;
    let changes = vec![update("1", "a", "A"), update("2", "b", "B"), delete("3"), insert("9", Some("i"))];
    let err = drv.apply_changes(schema, TABLE, &changes).await.expect_err("stale update must fail").to_string();
    assert_eq!(err, "change 1: row changed since it was loaded");
    assert_eq!(rows(drv).await, before, "nothing committed, and the connection is usable");
}

pub async fn delete_miss_and_db_error_roll_back(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    let before = rows(drv).await;
    let err = drv
        .apply_changes(schema, TABLE, &[update("1", "a", "A"), delete("99")])
        .await
        .expect_err("missing row must fail")
        .to_string();
    assert_eq!(err, "change 1: row no longer exists");
    assert_eq!(rows(drv).await, before);
    // A duplicate key is the database's own error, still tagged with the change index.
    let err = drv
        .apply_changes(schema, TABLE, &[delete("4"), insert("1", Some("dup"))])
        .await
        .expect_err("duplicate key must fail")
        .to_string();
    assert!(err.starts_with("change 1: "), "{err}");
    assert_eq!(rows(drv).await, before);
}
