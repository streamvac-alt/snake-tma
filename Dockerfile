# Этап сборки
FROM rust:1.85-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Создаем заглушку для кэширования зависимостей
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
# Копируем реальный код и пересобираем
COPY src ./src
COPY static ./static
RUN touch src/main.rs
RUN cargo build --release

# Финальный этап (маленький образ)
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/snake-tma .
COPY --from=builder /app/static ./static
ENV PORT=3000
EXPOSE 3000
CMD ["./snake-tma"] 
