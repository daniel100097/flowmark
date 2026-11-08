import { Router } from "express";
import rateLimit from "express-rate-limit";
import { forgotPassword, resetPassword } from "./controllers/password.js";
import { validate } from "./middleware/validate.js";
import { ForgotSchema, ResetSchema } from "./schemas.js";

export const router = Router();

router.post("/password/forgot", rateLimit({ windowMs: 60_000, max: 5 }), validate(ForgotSchema), forgotPassword);
router.post("/password/reset", validate(ResetSchema), resetPassword);
