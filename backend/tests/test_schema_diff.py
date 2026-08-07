from app.schema_models import ColumnDef, IndexDef, TableSchema
from app.schema_diff import diff_table


def _col(name, typ="int", nullable=False, default=None, comment=""):
    return ColumnDef(name=name, col_type=typ, nullable=nullable, default=default, comment=comment)


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
