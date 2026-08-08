# 高风险 DDL 与二次确认

日期：2026-08-08  
状态：已批准并实现

## 可执行

- 常规：`ALTER ADD/MODIFY`、`CREATE INDEX`
- 高风险：`INSERT` / `REPLACE`（含 `ON DUPLICATE KEY UPDATE`）、`DROP COLUMN/TABLE/INDEX`、`DELETE`、`UPDATE`、`TRUNCATE`
- 仍禁止：`DROP DATABASE`

## 二次确认

含高风险时：先确认说明 → 再输入「确认执行」。  
基准同步勾选 `dangerous` 差异时同样流程。
