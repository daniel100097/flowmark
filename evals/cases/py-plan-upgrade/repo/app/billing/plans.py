from sqlalchemy.orm import Session

from app.models import Plan


def list_public_plans(db: Session) -> list[Plan]:
    return db.query(Plan).filter(Plan.is_public.is_(True)).order_by(Plan.monthly_price_cents).all()


def get_plan(db: Session, plan_id: str) -> Plan | None:
    return db.get(Plan, plan_id)
