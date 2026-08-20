# ALTER TABLE 放开未知子句

日期：2026-08-20  
状态：已实现

## 踩坑记录（务必保留）

DDL 投放曾对 `ALTER TABLE` **子句做白名单**：未列出的子句一律报错「不支持的 ALTER 子句: …」，实际业务 SQL 频繁踩坑，例如：

| 现象 | 典型语句片段 |
|---|---|
| 报不支持 `CHANGE COLUMN` | `CHANGE COLUMN \`age_group_id\` \`category_id\` varchar(64) …` |
| 报不支持 `CONVERT TO CHARACTER SET` | `CONVERT TO CHARACTER SET utf8mb4 COLLATE utf8mb4_…` |
| 其它易漏 | `ENGINE=`、`AUTO_INCREMENT=`、`ADD PRIMARY KEY`、外键/`CHECK`、分区相关等 |

根因：用「枚举已知子句」追 MySQL 各版本语法，**永远追不全**。

## 现行策略

1. 识别为 `ALTER TABLE`（含可选 `ONLINE` / `OFFLINE` / `IGNORE`）后，**任意子句均可投放**。  
2. 风险分级：仅明确删除类（`DROP COLUMN/INDEX/KEY/…`、`DISCARD/IMPORT TABLESPACE` 等）→「修改表（删除列/索引等）」高风险档；其余 →「修改表（非删除类子句）」常规档。  
3. 未识别的**整句类型**（非已登记的 CREATE/ALTER/INSERT/…）仍拒绝；已登记类型可在「设置」里改常规 / 高风险 / 不允许。

不追求完整语法树解析；目标是「业务可投、风险可配」，避免再因白名单漏网拦截合法 `ALTER`。

## 相关代码

- `src-tauri/src/ddl_guard.rs`：`detect_alter_kinds` / `classify_alter_clause_kind`  
- `src-tauri/src/ddl_policy.rs`：策略文案与默认档位
