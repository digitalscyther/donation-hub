run local
```shell
docker compose build && POSTGRES_PORT="5438" RABBITMQ_PORT="5672" API_PORT="3001" docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d
```