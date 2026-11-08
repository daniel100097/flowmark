"""Billing v1. Replaced by app/billing in 1.0; kept for reference, not imported anywhere."""

import requests

STRIPE = "https://api.stripe.com/v1"


def charge(customer, amount):
    return requests.post(f"{STRIPE}/charges", data={"customer": customer, "amount": amount}).json()


def upgrade(db, user, plan):
    charge(user["stripe_id"], plan["price"])
    db.execute("UPDATE subscriptions SET plan = %s WHERE user_id = %s", (plan["id"], user["id"]))
