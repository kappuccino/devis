# Devis Groupe Cahors

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

## Documentation technique

L'écran **Documentation** (ex-application PDF Finder) recherche une référence produit dans les PDF
(texte de chaque page) et les images (nom du fichier) d'un dossier choisi dans
**Réglages → Documentation**. L'index (`docs-index.db`, à côté de `devis.db`) est un simple cache :
il est mis à jour en tâche de fond à chaque démarrage, seuls les fichiers nouveaux ou modifiés sont relus.

**Devis PDF + Docs** (éditeur de devis) : propose, pour chaque produit du devis (référence puis code
ENEDIS), les pages trouvées dans la documentation — correspondances exactes cochées par défaut — et
permet d'ajouter des documents pris sur l'ordinateur. Les choix sont mémorisés avec le devis
(table `quote_attachments`). Le PDF enchaîne le devis, une page de transition (sommaire avec numéros
de page) et les documents, dans l'ordre du devis (`src/docs/attachments.ts`). Les pages d'un PDF mal
formé, que pdf-lib ne sait pas recopier, sont rendues en image par pdfjs.

- Cœur (indexation, recherche, export de pages) : `src/docs/core/` (JS, repris de PDF Finder, avec ses tests).
- pdfjs (build legacy + worker) : `src/docs/pdfjs.ts` ; ses ressources sont copiées dans `public/pdfjs/`
  par `scripts/copy-pdfjs-assets.js` (lancé automatiquement avant `dev` et `build`).

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
2. construit les installeurs **Windows** (`.exe` NSIS et `.msi`), avec le fichier de mise à jour signé.
   La version **Mac** est construite en local (voir « Build local ») ;
3. publie une **release GitHub** :
   - à chaque push sur `main` : release `build-N` (N = numéro du run) ;
   - à chaque tag `v*` (ex. `git tag v0.2.0 && git push --tags`) : release officielle.

Penser à monter la version dans `src-tauri/tauri.conf.json` (et `package.json`) avant un tag.

### Mises à jour automatiques

Sous **Windows**, l'application installée cherche une nouvelle version à chaque démarrage (et dans
Réglages → À propos) : elle lit `latest.json` de la **dernière release officielle** du dépôt,
télécharge l'installeur signé, l'installe et redémarre. La version Mac, construite en local,
ne se met pas à jour toute seule.

- Les mises à jour sont signées avec la clé `~/.tauri/devis-groupe-cahors.key` (à sauvegarder :
  perdue, les applications installées ne peuvent plus être mises à jour). Sa clé publique est dans
  `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) ; la clé privée et son mot de passe sont dans
  les secrets du dépôt `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Les builds `build-N` (à chaque push) sont des **pré-versions** : jamais proposées en mise à jour.

**Publier une version** :

1. monter la version dans `src-tauri/tauri.conf.json` (et `package.json`), ex. `0.2.0` ;
2. commiter, puis poser le tag **avec les notes de version** (affichées dans la release et dans la
   fenêtre de mise à jour de l'appli) :
   `git tag -a v0.2.0 -m "Nouveautés : …" && git push && git push origin v0.2.0` ;
3. la CI construit, signe et publie la release `v0.2.0` avec `latest.json` (elle échoue si le tag
   ne correspond pas à la version).

### Build local (Mac)

```sh
npm run tauri build                     # « Devis Groupe Cahors.app » + .dmg
npm run tauri build -- --bundles app    # seulement l'app (plus rapide)
```

L'app est signée « ad-hoc » : au premier lancement, macOS demande de l'ouvrir via clic droit → Ouvrir.
Les fichiers de mise à jour signés ne sont produits que par la CI
(`--config src-tauri/tauri.updater.conf.json`, avec la clé de signature).
