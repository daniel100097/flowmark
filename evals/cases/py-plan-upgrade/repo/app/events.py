"""In-process event bus: maps event names to Celery tasks."""

from app.tasks.crm import sync_crm
from app.tasks.receipts import send_receipt

HANDLERS = {
    "plan.upgraded": [
        lambda p: send_receipt.delay(p["user_id"], p["invoice_id"]),
        lambda p: sync_crm.delay(p["user_id"]),
    ],
}


def publish(name: str, payload: dict) -> None:
    for handler in HANDLERS.get(name, []):
        handler(payload)
