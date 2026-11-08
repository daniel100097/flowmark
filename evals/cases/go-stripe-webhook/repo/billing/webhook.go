package billing

import (
	"encoding/json"
	"io"
	"log"
	"net/http"

	"github.com/stripe/stripe-go/v81/webhook"
)

type Handler struct {
	Secret  string
	Credits *Credits
	Subs    *Subscriptions
}

type invoice struct {
	Customer   string `json:"customer"`
	AmountPaid int64  `json:"amount_paid"`
}

type subscription struct {
	ID       string `json:"id"`
	Customer string `json:"customer"`
}

// StripeWebhook receives Stripe events. Stripe retries an event until it gets a 2xx answer.
func (h *Handler) StripeWebhook(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 1<<16))
	if err != nil {
		http.Error(w, "read error", http.StatusBadRequest)
		return
	}
	event, err := webhook.ConstructEvent(body, r.Header.Get("Stripe-Signature"), h.Secret)
	if err != nil {
		http.Error(w, "bad signature", http.StatusBadRequest)
		return
	}

	switch event.Type {
	case "invoice.paid":
		var inv invoice
		if err := json.Unmarshal(event.Data.Raw, &inv); err != nil {
			http.Error(w, "bad payload", http.StatusBadRequest)
			return
		}
		if err := h.Credits.Add(r.Context(), inv.Customer, inv.AmountPaid/100); err != nil {
			log.Printf("credit failed for %s: %v", inv.Customer, err)
		}
	case "customer.subscription.deleted":
		var sub subscription
		if err := json.Unmarshal(event.Data.Raw, &sub); err != nil {
			http.Error(w, "bad payload", http.StatusBadRequest)
			return
		}
		if err := h.Subs.Cancel(r.Context(), sub.ID); err != nil {
			http.Error(w, "cancel failed", http.StatusInternalServerError)
			return
		}
	}

	w.WriteHeader(http.StatusOK)
}
