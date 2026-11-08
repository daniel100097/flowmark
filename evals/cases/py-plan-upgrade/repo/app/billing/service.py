from datetime import datetime

from fastapi import HTTPException
from sqlalchemy.orm import Session

from app import events
from app.billing.invoices import create_invoice
from app.billing.plans import get_plan
from app.billing.proration import prorate
from app.models import Subscription, User
from app.payments import paddle


def upgrade_plan(db: Session, user: User, plan_id: str) -> dict:
    plan = get_plan(db, plan_id)
    if plan is None:
        raise HTTPException(status_code=404, detail="unknown plan")

    sub = db.query(Subscription).filter(Subscription.user_id == user.id).with_for_update().one()
    if sub.plan_id == plan.id:
        raise HTTPException(status_code=409, detail="already on this plan")

    now = datetime.utcnow()
    amount = prorate(sub.plan.monthly_price_cents, plan.monthly_price_cents, sub.period_ends_at, now)

    charge_id = ""
    if amount > 0:
        charge_id = paddle.charge(user.paddle_customer_id, amount, idempotency_key=f"upgrade-{sub.id}-{plan.id}")

    sub.plan_id = plan.id
    sub.status = "active"
    invoice = create_invoice(db, sub, amount, charge_id)
    db.commit()

    events.publish("plan.upgraded", {"user_id": user.id, "invoice_id": invoice.id, "plan_id": plan.id})
    return {"plan_id": plan.id, "status": sub.status, "charged_cents": amount}
