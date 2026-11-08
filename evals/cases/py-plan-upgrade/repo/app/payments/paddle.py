import os

import httpx

API = "https://api.paddle.com"


def charge(customer_id: str, amount_cents: int, idempotency_key: str) -> str:
    """Charges the customer's saved payment method. Returns the charge id."""
    resp = httpx.post(
        f"{API}/charges",
        headers={"Authorization": f"Bearer {os.environ['PADDLE_API_KEY']}", "Idempotency-Key": idempotency_key},
        json={"customer_id": customer_id, "amount": amount_cents, "currency": "EUR"},
        timeout=10,
    )
    resp.raise_for_status()
    return resp.json()["data"]["id"]
