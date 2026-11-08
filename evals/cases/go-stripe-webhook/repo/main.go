package main

import (
	"context"
	"log"
	"net/http"
	"os"

	"example.com/billing/billing"
	"github.com/jackc/pgx/v5/pgxpool"
)

func main() {
	db, err := pgxpool.New(context.Background(), os.Getenv("DATABASE_URL"))
	if err != nil {
		log.Fatal(err)
	}
	h := &billing.Handler{
		Secret:  os.Getenv("STRIPE_WEBHOOK_SECRET"),
		Credits: &billing.Credits{DB: db},
		Subs:    &billing.Subscriptions{DB: db, Mail: billing.NewMailer(os.Getenv("SENDGRID_KEY"))},
	}

	mux := http.NewServeMux()
	mux.HandleFunc("POST /webhooks/stripe", h.StripeWebhook)
	log.Fatal(http.ListenAndServe(":8080", mux))
}
