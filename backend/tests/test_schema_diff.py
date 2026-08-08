from app.schema_models import ColumnDef, IndexDef, TableSchema
from app.schema_diff import diff_table


def _col(name, typ="int", nullable=False, default=None, comment=""):
    return ColumnDef(name=name, col_type=typ, nullable=nullable, default=default, comment=comment)


def _idx(name, columns, *, unique=False, primary=False):
    return IndexDef(name=name, columns=columns, unique=unique, primary=primary)


def test_missing_column_is_safe_add():
    tmpl = TableSchema(name="t", columns=[_col("id"), _col("name", "varchar(64)")], indexes=[], create_sql="CREATE TABLE t (id int, name varchar(64))")
    tgt = TableSchema(name="t", columns=[_col("id")], indexes=[])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    kinds = [i.kind for i in items]
    assert "add_column" in kinds
    add = next(i for i in items if i.kind == "add_column")
    assert add.risk == "safe" and add.selected_default is True
    assert "ADD COLUMN" in add.sql.upper()


def test_extra_column_dangerous_not_selected():
    tmpl = TableSchema(name="t", columns=[_col("id")], indexes=[])
    tgt = TableSchema(name="t", columns=[_col("id"), _col("legacy")], indexes=[])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    drop = next(i for i in items if i.kind == "drop_column")
    assert drop.risk == "dangerous" and drop.selected_default is False


def test_missing_table_create():
    tmpl = TableSchema(name="t", columns=[_col("id")], indexes=[], create_sql="CREATE TABLE `t` (`id` int)")
    items = diff_table(tmpl, None, instance_id="main", database="db1")
    assert len(items) == 1 and items[0].kind == "create_table"
    assert items[0].sql.startswith("CREATE TABLE")


def test_modify_column_caution_not_selected():
    tmpl = TableSchema(name="t", columns=[_col("id", "bigint")], indexes=[])
    tgt = TableSchema(name="t", columns=[_col("id", "int")], indexes=[])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    mod = next(i for i in items if i.kind == "modify_column")
    assert mod.risk == "caution" and mod.selected_default is False
    assert "MODIFY COLUMN" in mod.sql.upper()


def test_primary_key_name_only_diff_is_noop():
    cols = [_col("id")]
    tmpl = TableSchema(name="t", columns=cols, indexes=[_idx("PRIMARY", ["id"], primary=True)])
    tgt = TableSchema(name="t", columns=cols, indexes=[_idx("pk_id", ["id"], primary=True)])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    assert items == []


def test_non_primary_index_change_is_drop_then_add_caution():
    cols = [_col("id"), _col("name", "varchar(64)")]
    tmpl = TableSchema(
        name="t",
        columns=cols,
        indexes=[_idx("idx_name", ["name"], unique=True)],
    )
    tgt = TableSchema(
        name="t",
        columns=cols,
        indexes=[_idx("idx_name", ["name"], unique=False)],
    )
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    kinds = [i.kind for i in items]
    assert kinds == ["drop_index", "add_index"]
    drop, add = items
    assert drop.risk == "dangerous" and drop.selected_default is False
    assert add.risk == "caution" and add.selected_default is False


def test_standalone_missing_index_add_is_safe():
    cols = [_col("id"), _col("name", "varchar(64)")]
    tmpl = TableSchema(name="t", columns=cols, indexes=[_idx("idx_name", ["name"])])
    tgt = TableSchema(name="t", columns=cols, indexes=[])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    add = next(i for i in items if i.kind == "add_index")
    assert add.risk == "safe" and add.selected_default is True


def test_diff_item_id_format():
    tmpl = TableSchema(name="t", columns=[_col("id"), _col("name", "varchar(64)")], indexes=[])
    tgt = TableSchema(name="t", columns=[_col("id")], indexes=[])
    items = diff_table(tmpl, tgt, instance_id="inst-1", database="shop_db")
    add = next(i for i in items if i.kind == "add_column")
    assert add.id == "inst-1|shop_db|t|add_column|name"


def test_table_comment_diff_is_caution_alter():
    cols = [_col("id")]
    tmpl = TableSchema(name="t", columns=cols, indexes=[], comment="模板注释")
    tgt = TableSchema(name="t", columns=cols, indexes=[], comment="旧注释")
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    assert len(items) == 1
    item = items[0]
    assert item.kind == "modify_table"
    assert item.risk == "caution"
    assert item.selected_default is False
    assert "ALTER TABLE" in item.sql.upper()
    assert "COMMENT=" in item.sql.upper()
    assert "模板注释" in item.sql


def test_table_comment_equal_no_diff():
    cols = [_col("id")]
    tmpl = TableSchema(name="t", columns=cols, indexes=[], comment="同")
    tgt = TableSchema(name="t", columns=cols, indexes=[], comment="同")
    assert diff_table(tmpl, tgt, instance_id="main", database="db1") == []
