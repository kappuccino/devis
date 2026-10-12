# Service de licences et de mises à jour de l'appli Devis (déployé par Coolify).
FROM node:26-alpine

ENV NODE_ENV=production \
    PORT=3000 \
    DATA_DIR=/data

# su-exec : lancer le service sous l'utilisateur node après avoir réglé les droits de /data.
RUN apk add --no-cache su-exec

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev && npm cache clean --force
COPY src ./src
COPY scripts ./scripts

# Installeurs et latest.json : dossier monté par Coolify (droits réglés au démarrage).
RUN mkdir -p /data && chown node:node /data
VOLUME /data

EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+process.env.PORT+'/health').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"

ENTRYPOINT ["sh", "/app/scripts/entrypoint.sh"]
CMD ["node", "src/index.js"]
