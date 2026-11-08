from pydantic import BaseModel, Field


class UpgradeRequest(BaseModel):
    plan_id: str = Field(min_length=1, max_length=64)


class UpgradeResponse(BaseModel):
    plan_id: str
    status: str
    charged_cents: int
