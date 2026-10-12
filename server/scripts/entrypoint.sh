#!/bin/sh
# Démarrage du conteneur : le dossier des installeurs (/data, monté depuis le serveur) est rendu à
# l'utilisateur node, puis le service est lancé sous cet utilisateur (jamais en root).
set -e
if [ "$(id -u)" = "0" ]; then
  mkdir -p "$DATA_DIR"
  chown -R node:node "$DATA_DIR"
  exec su-exec node "$@"
fi
exec "$@"
