package billing

import (
	"context"
	"log"

	"github.com/jackc/pgx/v5/pgxpool"
)

type Credits struct {
	DB *pgxpool.Pool
}

// Add credits a customer's account and records the change in the ledger.
func (c *Credits) Add(ctx context.Context, stripeCustomer string, amount int64) error {
	tx, err := c.DB.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	var accountID int64
	var email string
	err = tx.QueryRow(ctx,
		`UPDATE accounts SET credits = credits + $1 WHERE stripe_customer = $2 RETURNING id, email`,
		amount, stripeCustomer,
	).Scan(&accountID, &email)
	if err != nil {
		return err
	}
	if _, err := tx.Exec(ctx,
		`INSERT INTO credit_ledger (account_id, amount, reason) VALUES ($1, $2, 'invoice.paid')`,
		accountID, amount,
	); err != nil {
		return err
	}
	log.Printf("credited %d to account %d (%s)", amount, accountID, email)
	return tx.Commit(ctx)
}
