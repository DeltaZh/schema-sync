from app.schema_models import ColumnDef
from app.sql_gen import add_column_sql


def test_add_column_sql():
    sql = add_column_sql("t", ColumnDef(name="name", col_type="varchar(64)", nullable=True, default=None, comment="n"))
    assert "ALTER TABLE `t` ADD COLUMN `name` varchar(64)" in sql
