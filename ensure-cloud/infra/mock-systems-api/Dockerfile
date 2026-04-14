# syntax=docker/dockerfile:1.6

FROM node:20.20.2-alpine3.23
WORKDIR /app

COPY package*.json ./
RUN npm install --omit=dev

COPY server.js ./server.js

ENV PORT=8081

EXPOSE 8081

CMD ["node", "server.js"]
