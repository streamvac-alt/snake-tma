# --- Этап сборки ---
FROM rust:1.85-slim AS builder
WORKDIR /usr/src/app

# Копируем ВСЕ файлы из папки проекта в контейнер
COPY . .

# Собираем проект
RUN cargo build --release

# --- Финальный этап ---
FROM debian:bookworm-slim
WORKDIR /app

# Устанавливаем сертификаты (нужны для работы сети)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Копируем скомпилированный бинарник из первого этапа
COPY --from=builder /usr/src/app/target/release/snake-tma .

# Копируем папку со статикой (index.html)
COPY --from=builder /usr/src/app/static ./static

# Открываем порт и запускаем
ENV PORT=3000
EXPOSE 3000
CMD ["./snake-tma"]
