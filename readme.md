run local
```shell
POSTGRES_PORT="5438" docker compose build && docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d
```