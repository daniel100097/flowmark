import { randomBytes } from "node:crypto";
import bcrypt from "bcrypt";
import type { Request, Response } from "express";
import { db } from "../lib/db.js";
import { mailer } from "../lib/mailer.js";

const APP_URL = process.env.APP_URL ?? "http://localhost:3000";
const ONE_HOUR = 60 * 60 * 1000;

export async function forgotPassword(req: Request, res: Response) {
  const { email } = req.body;
  const user = await db.user.findUnique({ where: { email } });
  if (!user) {
    // Same answer whether or not the address exists.
    return res.json({ ok: true });
  }

  const token = randomBytes(32).toString("hex");
  await db.passwordReset.create({ data: { userId: user.id, token, expiresAt: new Date(Date.now() + ONE_HOUR) } });

  await mailer.send(user.email, "password-reset", { link: `${APP_URL}/reset?token=${token}` });
  res.json({ ok: true });
}

export async function resetPassword(req: Request, res: Response) {
  const { token, password } = req.body;
  const reset = await db.passwordReset.findFirst({ where: { token, expiresAt: { gt: new Date() } }, include: { user: true } });
  if (!reset) {
    return res.status(400).json({ error: "invalid or expired token" });
  }

  const passwordHash = await bcrypt.hash(password, 10);
  await db.user.update({ where: { id: reset.userId }, data: { passwordHash } });

  await mailer.send(reset.user.email, "password-changed", { name: reset.user.name, password });
  res.json({ ok: true });
}
