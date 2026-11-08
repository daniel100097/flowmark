from fastapi import Depends, Header, HTTPException
from sqlalchemy.orm import Session

from app.db import SessionLocal
from app.models import ApiToken, User


def get_db():
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()


def get_current_user(authorization: str = Header(...), db: Session = Depends(get_db)) -> User:
    token = authorization.removeprefix("Bearer ").strip()
    row = db.query(ApiToken).filter(ApiToken.token == token, ApiToken.revoked.is_(False)).one_or_none()
    if row is None:
        raise HTTPException(status_code=401, detail="invalid token")
    return row.user


def require_admin(user: User = Depends(get_current_user)) -> User:
    if not user.is_admin:
        raise HTTPException(status_code=403)
    return user
