# Build stage
FROM python:3.11-slim as builder

WORKDIR /build
COPY ./setup.py ./MANIFEST.in ./README.md ./
COPY ./ailets/. ./ailets/
COPY --from=ailets-wasm-actors /dist/*.wasm ./ailets/wasm/

# Install build dependencies and build the wheel
RUN pip install --no-cache-dir build wheel \
    && python -m build --wheel

# Production stage
FROM python:3.11-slim

WORKDIR /app

# Copy and install the wheel from builder stage
COPY --from=builder /build/dist/*.whl .
RUN pip install --no-cache-dir *.whl \
    && rm *.whl

# Verify installation
RUN python -c "import ailets; print(f'ailets version {ailets.__version__}')"

CMD ["python", "-m", "ailets"]
