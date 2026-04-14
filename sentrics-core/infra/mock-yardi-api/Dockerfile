FROM node:20.20.2-alpine3.23

WORKDIR /app

COPY package*.json ./
RUN npm ci --only=production

COPY server.js ./

EXPOSE 3000

CMD ["npm", "start"]
