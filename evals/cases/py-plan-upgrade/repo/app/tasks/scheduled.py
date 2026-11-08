from datetime import datetime

from app.celery_app import celery
from app.db import SessionLocal
from app.models import Subscription


@celery.task
def expire_trials() -> int:
    """Moves subscriptions whose trial is over back to the free plan."""
    with SessionLocal() as db:
        expired = (
            db.query(Subscription)
            .filter(Subscription.trial_ends_at.is_not(None), Subscription.trial_ends_at < datetime.utcnow())
            .all()
        )
        for sub in expired:
            sub.plan_id = "free"
            sub.status = "expired"
        db.commit()
        return len(expired)
