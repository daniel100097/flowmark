import smtplib
from email.message import EmailMessage

from app.celery_app import celery
from app.db import SessionLocal
from app.models import Invoice, User


@celery.task(autoretry_for=(smtplib.SMTPException,), retry_backoff=True, max_retries=5)
def send_receipt(user_id: int, invoice_id: int) -> None:
    with SessionLocal() as db:
        user = db.get(User, user_id)
        invoice = db.get(Invoice, invoice_id)
        msg = EmailMessage()
        msg["To"] = user.email
        msg["From"] = "billing@example.com"
        msg["Subject"] = f"Receipt #{invoice.id}"
        msg.set_content(f"You were charged {invoice.amount_cents / 100:.2f} EUR.")
        with smtplib.SMTP("smtp.internal", 25) as smtp:
            smtp.send_message(msg)
