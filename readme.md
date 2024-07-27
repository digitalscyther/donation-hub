run local
```shell
docker compose build && POSTGRES_PORT="5438" RABBITMQ_PORT="5672" REDIS_PORT="6381" API_PORT="3001" TRANSACTIONS_PORT="3002" HDWALLET_PORT="8001" docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d
```