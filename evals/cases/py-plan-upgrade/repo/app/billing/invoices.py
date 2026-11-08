from sqlalchemy.orm import Session

from app.models import Invoice, Subscription


def create_invoice(db: Session, sub: Subscription, amount_cents: int, charge_id: str) -> Invoice:
    invoice = Invoice(subscription_id=sub.id, amount_cents=amount_cents, paddle_charge_id=charge_id)
    db.add(invoice)
    return invoice
