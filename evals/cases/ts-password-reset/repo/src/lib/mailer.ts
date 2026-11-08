const API = "https://api.postmarkapp.com/email/withTemplate";

export const mailer = {
  async send(to: string, template: string, model: Record<string, unknown>) {
    await fetch(API, {
      method: "POST",
      headers: { "X-Postmark-Server-Token": process.env.POSTMARK_TOKEN ?? "", "Content-Type": "application/json" },
      body: JSON.stringify({ To: to, From: "accounts@example.com", TemplateAlias: template, TemplateModel: model }),
    });
  },
};
