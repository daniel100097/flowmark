from datetime import datetime


def prorate(old_cents: int, new_cents: int, period_ends_at: datetime, now: datetime, days_in_period: int = 30) -> int:
    """Price difference for the rest of the current period, in cents (never negative)."""
    days_left = max(0, (period_ends_at - now).days)
    return max(0, (new_cents - old_cents) * days_left // days_in_period)
