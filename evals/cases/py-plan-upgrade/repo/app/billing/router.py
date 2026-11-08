from fastapi import APIRouter, Depends
from sqlalchemy.orm import Session

from app.billing import plans
from app.billing.schemas import UpgradeRequest, UpgradeResponse
from app.billing.service import upgrade_plan
from app.deps import get_current_user, get_db
from app.models import User

router = APIRouter()


@router.get("/plans")
def public_plans(db: Session = Depends(get_db)):
    return [{"id": p.id, "name": p.name, "price": p.monthly_price_cents} for p in plans.list_public_plans(db)]


@router.post("/upgrade", response_model=UpgradeResponse)
def upgrade(body: UpgradeRequest, user: User = Depends(get_current_user), db: Session = Depends(get_db)):
    return upgrade_plan(db, user, body.plan_id)
