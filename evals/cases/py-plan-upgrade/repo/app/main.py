from fastapi import FastAPI

from app.admin.reports import router as admin_router
from app.billing.router import router as billing_router
from app.users.router import router as users_router

app = FastAPI(title="Billing")
app.include_router(users_router, prefix="/users")
app.include_router(billing_router, prefix="/billing")
app.include_router(admin_router, prefix="/admin")
