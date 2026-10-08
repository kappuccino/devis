# Devis Cahors

Application de bureau (Tauri 2 + Vue 3 + SQLite) pour créer des devis à partir du classeur « LPN finale.xlsx ».

## Lancer

```sh
npm install
npm run tauri dev      # développement
npm run tauri build    # application installable
```

La base SQLite est stockée dans le dossier de données de l'utilisateur :
- macOS : `~/Library/Application Support/fr.devis.app/devis.db`
- Windows : `%APPDATA%\fr.devis.app\devis.db`
Au premier lancement, aller dans **Réglages → Importer un fichier LPN…**.

## Données importées

| Feuille                    | Contenu                                                                          |
| -------------------------- | -------------------------------------------------------------------------------- |
| `DATA`                     | Produits : réf (A), désignation (C), prix public (D), famille CFA/CFO (G), prix seuil (L) |
| `gestion tarifs`           | Clients : code (B), nom (A), remise CFA (P), remise CFO (R), listes de prix (I, K, L, M) |
| `fichier injection tarif`  | Listes de prix nets : code liste (D), réf (E), prix (O)                         |

Un réimport remplace produits, clients et listes (y compris les modifications faites dans l'appli). Les devis ne sont pas touchés.

## Règle de prix

Pour un client et une référence :
1. le prix le plus bas parmi les listes de prix rattachées au client (les dates de validité sont ignorées) ;
2. sinon, le prix public moins la remise CFA ou CFO du client, selon la famille du produit.

Le prix reste modifiable sur la ligne. Un avertissement s'affiche s'il passe sous le prix seuil.

## Tests

```sh
npm test                                                       # calculs des lignes, remises, déplacements
cd src-tauri
cargo test                                                     # prix, import, migrations
LPN_XLSX="../documents-source/LPN finale .xlsx" cargo test -- --ignored --nocapture   # import réel
```

## Release

Le workflow GitHub Actions (`.github/workflows/build.yml`) :

1. lance les tests (types, vitest, `cargo test`) ;
2. construit les installeurs **macOS universel** (`.dmg`) et **Windows** (`.exe` NSIS et `.msi`) ;
3. publie une **release GitHub** :
   - à chaque push sur `main` : release `build-N` (N = numéro du run) ;
   - à chaque tag `v*` (ex. `git tag v0.2.0 && git push --tags`) : release officielle.

Penser à monter la version dans `src-tauri/tauri.conf.json` (et `package.json`) avant un tag.

### Signature macOS

Sans secret, l'app est signée « ad-hoc » : au premier lancement, macOS demande de l'ouvrir via
clic droit → Ouvrir. Pour une signature Developer ID et la notarisation, ajouter dans les secrets
du dépôt : `APPLE_CERTIFICATE` (.p12 en base64), `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (mot de passe d'app), `APPLE_TEAM_ID`.

### Build local

```sh
npm run tauri build                     # installeurs de la machine courante
npm run tauri build -- --bundles app    # seulement « Devis Cahors.app » (plus rapide)
```
