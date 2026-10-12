# Service de licences et de mises à jour

`licence.cahors.kappuccino.net` : Node + Express (JavaScript), comptes et postes dans PocketBase.
Déployé par Coolify à partir du `Dockerfile` de ce dossier.

## Rôle

- **Licences** : l'appli se connecte (email + mot de passe d'un compte PocketBase) et reçoit un
  ticket signé (Ed25519) valable `LICENCE_DAYS` jours ; elle le renouvelle à chaque lancement avec
  internet, avec le secret de son poste. Hors ligne, elle fonctionne jusqu'à l'expiration du ticket.
- **Mises à jour** : la CI dépose installeurs et `latest.json` ; seuls les postes munis d'un ticket
  valide peuvent les télécharger.

## Routes

| Route | Qui | Rôle |
|---|---|---|
| `POST /api/licence/activate` | appli | `{ email, password, machine_id, machine_name, app_version }` → `{ ticket, device_id, secret }` |
| `POST /api/licence/renew` | appli | `{ device_id, secret, app_version }` → `{ ticket }` |
| `POST /api/licence/deactivate` | appli | `{ device_id, secret }` : libère le poste |
| `GET /api/public-key` | tous | clé publique des tickets (celle embarquée dans l'appli) |
| `PUT /api/releases/<version>/<fichier>` | CI | dépôt d'un installeur (`Authorization: Bearer RELEASE_UPLOAD_TOKEN`) |
| `PUT /api/releases/latest.json` | CI | manifeste, à déposer en dernier |
| `GET /updates/latest.json`, `GET /updates/<version>/<fichier>` | appli | `Authorization: Bearer <ticket>` |
| `GET /health` | Docker | contrôle de santé |

Erreurs : `{ "error": "<message en français, affichable tel quel>" }`.

`machine_id` : empreinte SHA-256 (64 caractères hexadécimaux) calculée par l'appli à partir de
l'identifiant de la machine. Ticket : `<données base64url>.<signature base64url>`, données
`{ v, sub, email, name, device, machine, iat, exp }`.

## PocketBase

Au démarrage, le service crée ou complète les tables (rien n'est supprimé) :

- `users` : champs ajoutés `active` (à cocher pour autoriser le compte) et `max_devices` (vide :
  `DEFAULT_MAX_DEVICES`) ; inscription et accès directs fermés.
- `devices` : postes (`machine_name`, `app_version`, `last_seen`) ; cocher `revoked` bloque un
  poste, le supprimer libère sa place.

Administration (interface PocketBase) : créer un compte (email, mot de passe, nom, `active`),
décocher `active` pour couper l'accès (effectif au plus tard à l'expiration du ticket).

## Déploiement (Coolify)

1. Clé des tickets, une seule fois, hors du dépôt : `npm run keygen -- licence.pem`. La clé privée
   va dans Coolify (fichier monté ou variable `LICENCE_PRIVATE_KEY`), la clé publique dans l'appli.
   **Ne jamais la régénérer** : les postes ne pourraient plus vérifier leurs tickets.
2. Superutilisateur PocketBase réservé au service. Le MFA s'appliquant à tous les superutilisateurs
   (règle non modifiable sur `_superusers`), le service utilise un jeton fixe : interface PocketBase →
   `_superusers` → compte du service → **Impersonate** (durée longue) → variable `PB_ADMIN_TOKEN`.
   Changer le mot de passe de ce compte invalide le jeton.
3. Variables : voir `.env.example`. Volume persistant sur `/data` (installeurs).
4. Domaine `https://licence.cahors.kappuccino.net:3000`.

## Tests

```
npm test
# avec un vrai PocketBase vide :
PB_TEST_URL=http://127.0.0.1:8099 PB_TEST_EMAIL=… PB_TEST_PASSWORD=… npm test
```
