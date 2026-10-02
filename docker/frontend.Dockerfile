# LinkFYR frontend verification image: typecheck, lint, test, build.
FROM node:24-bookworm-slim

RUN corepack enable && corepack prepare pnpm@12.8.1 --activate

WORKDIR /app
COPY . .

RUN pnpm install --frozen-lockfile

CMD ["bash", "scripts/docker/frontend-gate.sh"]
