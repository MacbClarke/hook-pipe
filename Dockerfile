# Build stage
FROM rust:1.90-alpine AS builder

WORKDIR /app

RUN apk add pkgconfig libressl-dev musl-dev

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application with musl target for static linking
RUN cargo build --release

# Runtime stage
FROM alpine:latest

WORKDIR /app

# Install CA certificates for HTTPS requests
RUN apk add --no-cache ca-certificates

# Copy the built binary from builder stage
COPY --from=builder /app/target/release/hook-pipe .

# Expose the default port
EXPOSE 8080

# Set environment variables
ENV RUST_LOG=info

# Run the application
CMD ["./hook-pipe"]
