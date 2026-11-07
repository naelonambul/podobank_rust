FROM python:3.14-slim
WORKDIR /app
COPY backend/requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY backend/app ./app
COPY backend/migrations ./migrations
COPY lab/compose/check.py /checks/check.py
COPY lab/compose/business.py /checks/business.py
COPY lab/compose/faults.py /checks/faults.py
CMD ["uvicorn", "app.main:create_app", "--factory", "--host", "0.0.0.0", "--port", "8000"]
