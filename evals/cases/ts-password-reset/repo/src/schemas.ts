import { z } from "zod";

export const ForgotSchema = z.object({
  email: z.string().email(),
});

export const ResetSchema = z.object({
  token: z.string().min(1),
  password: z.string().min(8).max(128),
});
