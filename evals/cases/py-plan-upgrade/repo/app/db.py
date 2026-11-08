from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker

engine = create_engine("postgresql+psycopg://billing@db/billing")
SessionLocal = sessionmaker(bind=engine, expire_on_commit=False)
