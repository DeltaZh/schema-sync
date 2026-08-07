from pydantic import BaseModel, Field


class InstanceConfig(BaseModel):
    id: str
    host: str
    port: int = 3306
    user: str
    password: str = ""
    enabled: bool = True
    remark: str = ""


class TableGroupConfig(BaseModel):
    id: str
    database_pattern: str
    tables: list[str] = Field(default_factory=list)
    instance_ids: list[str] = Field(default_factory=list)


class AppConfig(BaseModel):
    instances: list[InstanceConfig] = Field(default_factory=list)
    table_groups: list[TableGroupConfig] = Field(default_factory=list)
