# 高风险 DDL 与二次确认

日期：2026-08-08（2026-08-19 补充可配置策略）  
状态：已实现

## 可执行（默认策略）

- 常规：`CREATE TABLE`、`ALTER ADD/MODIFY/COMMENT`、`CREATE INDEX`
- 高风险：`INSERT` / `REPLACE`（含 `ON DUPLICATE KEY UPDATE`）、`DROP COLUMN/TABLE/INDEX`、`DELETE`、`UPDATE`、`TRUNCATE`
- 默认不允许：`DROP DATABASE` / `DROP SCHEMA`（可在「设置」改为高风险或常规）

未识别的语句类型仍拒绝。策略存本机 `config.json` 的 `ddl_policy`，仅影响 DDL 投放。

## 二次确认

含高风险时：先确认说明 → 再输入「确认执行」。  
基准同步勾选 `dangerous` 差异时同样流程。
