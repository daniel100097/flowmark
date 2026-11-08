from fastapi import APIRouter, Depends
from sqlalchemy import func
from sqlalchemy.orm import Session

from app.deps import get_db, require_admin
from app.models import Invoice

router = APIRouter()


@router.get("/revenue")
def revenue(db: Session = Depends(get_db), _admin=Depends(require_admin)):
    total = db.query(func.coalesce(func.sum(Invoice.amount_cents), 0)).scalar()
    return {"total_cents": total}
