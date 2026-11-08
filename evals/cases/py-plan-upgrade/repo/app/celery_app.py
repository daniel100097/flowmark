from celery import Celery
from celery.schedules import crontab

celery = Celery("billing", broker="redis://redis:6379/0", include=["app.tasks.receipts", "app.tasks.crm", "app.tasks.scheduled"])

celery.conf.beat_schedule = {
    "expire-trials": {"task": "app.tasks.scheduled.expire_trials", "schedule": crontab(minute=0)},
}
