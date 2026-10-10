# Devis Groupe Cahors — consignes pour Claude

Appli de bureau Tauri 2 + Vue 3 / TypeScript + PrimeVue + SQLite (voir README.md). On échange en français.

## Page Aide : à tenir à jour (obligatoire)

La page **Aide** (`src/views/HelpView.vue`, contenu dans `src/help.ts`) liste toutes les
fonctionnalités de l'appli, par rubrique. **À chaque ajout ou modification d'une fonctionnalité
visible** (nouvel écran, bouton, raccourci clavier, réglage, comportement qui change), mettre à jour
`src/help.ts` dans le même travail, sans attendre qu'on le demande :
- ajouter ou corriger l'entrée concernée dans la bonne rubrique (`title`, `text`, `keys` pour les raccourcis) ;
- retirer les entrées des fonctionnalités supprimées ;
- rédiger pour l'utilisateur (ce que ça fait et où le trouver), pas pour le développeur.

## Autres règles

- Ne jamais commiter sans demande explicite ; ne jamais pousser (branche ou tags) sans « push » explicite.
- Commits séparés par sujet, messages en français.
- Les données clients (`documents-source/`, bases `.db`, xlsx) ne vont jamais dans git ; ne jamais lire
  ni afficher la clé de signature privée.
- Tenir `TODO.md` à jour (cocher ce qui est fait).
- Accès aux fichiers depuis la page : uniquement via `src/dialogs.ts` (boîtes de dialogue côté Rust,
  `src-tauri/src/files.rs`) ; une nouvelle commande qui lit ou écrit un fichier vérifie le chemin avec
  `FileAccess`.
- Nouvelle version : le numéro est dans `package.json`, `package-lock.json` (2 endroits),
  `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` et `src-tauri/Cargo.lock` ; tag annoté `vX.Y.Z`
  dont le message sert de notes de version.
