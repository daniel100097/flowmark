package billing

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"
)

type Subscriptions struct {
	DB   *pgxpool.Pool
	Mail *Mailer
}

// Cancel marks a subscription as canceled and tells the customer.
func (s *Subscriptions) Cancel(ctx context.Context, stripeSubscription string) error {
	var email string
	err := s.DB.QueryRow(ctx,
		`UPDATE subscriptions SET status = 'canceled', canceled_at = now() WHERE stripe_id = $1 RETURNING email`,
		stripeSubscription,
	).Scan(&email)
	if err != nil {
		return err
	}
	return s.Mail.Send(ctx, email, "subscription-canceled")
}
