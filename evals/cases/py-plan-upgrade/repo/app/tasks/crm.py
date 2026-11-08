import os

import httpx

from app.celery_app import celery
from app.db import SessionLocal
from app.models import Subscription, User

HUBSPOT = "https://api.hubapi.com/crm/v3/objects/contacts"


@celery.task(max_retries=3)
def sync_crm(user_id: int) -> None:
    with SessionLocal() as db:
        user = db.get(User, user_id)
        sub = db.query(Subscription).filter(Subscription.user_id == user_id).one()
        properties = {**user.to_dict(), "plan": sub.plan_id}
        httpx.post(
            HUBSPOT,
            headers={"Authorization": f"Bearer {os.environ['HUBSPOT_TOKEN']}"},
            json={"properties": properties},
            timeout=10,
        )
