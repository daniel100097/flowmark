package billing

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
)

type Mailer struct {
	key string
}

func NewMailer(key string) *Mailer { return &Mailer{key: key} }

// Send sends a SendGrid dynamic template mail.
func (m *Mailer) Send(ctx context.Context, to, template string) error {
	body, _ := json.Marshal(map[string]any{
		"personalizations": []map[string]any{{"to": []map[string]string{{"email": to}}}},
		"from":             map[string]string{"email": "billing@example.com"},
		"template_id":      template,
	})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, "https://api.sendgrid.com/v3/mail/send", bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+m.key)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	return resp.Body.Close()
}
