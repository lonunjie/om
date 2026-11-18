# SPDX-License-Identifier: AGPL-3.0-or-later
# Build from the OM repository root using -f infra/sqlite-home/Dockerfile.
FROM node:24-bookworm-slim AS build
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY infra/sqlite-home/package.json infra/sqlite-home/
COPY packages/om-home-wire/package.json packages/om-home-wire/
RUN corepack pnpm --filter @om/sqlite-home... install --frozen-lockfile
COPY packages/om-home-wire/src packages/om-home-wire/src
COPY infra/cloudflare-home/src infra/cloudflare-home/src
COPY infra/sqlite-home/src infra/sqlite-home/src
COPY infra/sqlite-home/build.mjs infra/sqlite-home/
COPY LICENSES LICENSES
RUN corepack pnpm --filter @om/sqlite-home build

FROM node:24-bookworm-slim
WORKDIR /opt/om
COPY --from=build /src/infra/sqlite-home/dist/home.mjs ./
COPY --from=build /src/LICENSES ./LICENSES
COPY --from=build /src/infra/sqlite-home/node_modules/ws/LICENSE ./LICENSES/ws-MIT.txt
RUN install -d -m 0700 -o node -g node /var/lib/om-home
ENV OM_DATA_DIR=/var/lib/om-home OM_HOST=0.0.0.0 OM_PORT=8787
USER node
VOLUME ["/var/lib/om-home"]
EXPOSE 8787
CMD ["node", "home.mjs"]
