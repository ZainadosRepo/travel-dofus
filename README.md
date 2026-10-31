# Travel Dofus

Petite fenêtre toujours au-dessus de Dofus 3 : on cherche un PNJ, une zone, un zaap, un repère de la carte (banque, atelier…) ou une position `x,y`, et l'appli copie le trajet le plus court (`/zaap x,y` répété par l'anti lag, puis `/travel x,y`). Avec « Ma position », elle compare la marche directe et le passage par un zaap.

Les données (2 080 PNJ placés par les quêtes, 47 zaaps, 834 repères, 15 409 cartes et leurs liaisons, noms dans les 5 langues du jeu) sont extraites des fichiers du client installé et embarquées dans l'appli. L'interface reprend les 11 thèmes et la police (Lexend) de l'interface de Dofus 3.

Projet de fan non officiel, sans lien avec Ankama. Dofus est une marque d'Ankama Games ; les données et les couleurs affichées proviennent de son client.

## Utilisation

| Action | Comment |
| --- | --- |
| Afficher / masquer la fenêtre | Ctrl + Alt + P, même depuis le jeu |
| Copier le résultat sélectionné | Entrée, ou clic sur la carte |
| Changer ce qui est copié (trajet, /zaap + /travel, /travel, /zaap, position, nom) | Tab / Maj + Tab, ou le bouton « Copie » |
| Favori (6 par profil) | ☆ sur un résultat ; recherche vide = liste des favoris, glisser pour réordonner |
| Copier un favori | Alt + 1 à 6 (ou 1 à 6 hors du champ de recherche) |
| Corriger un PNJ, choisir son zaap, rétablir | clic droit ou ⋯ sur un résultat |
| Profils, positions perso, anti lag, opacité, langue, thème | ⚙ |

Les réglages sont dans `settings.json` du dossier de configuration de l'appli ; la taille et la position de la fenêtre sont retenues.

## Installation

- Windows : installeur `.exe` (NSIS) des releases GitHub.
- macOS : image `.dmg` des releases. Non signée : clic droit > Ouvrir au premier lancement.

Les releases sont construites par `.github/workflows/release.yml` (tag `v*` ou lancement manuel), en brouillon.

## Lancer en développement

Prérequis : Rust stable, Node (pour les tests de l'interface) et la CLI Tauri 2 (`cargo install tauri-cli --version "^2" --locked`).

```bash
cargo tauri dev
```

### Après un patch de Dofus

L'appli affiche un bandeau quand la version installée diffère de celle des données. Pour mettre à jour :

```bash
cargo run --release -p dofus-data --bin extract
```

Cela relit l'install (`%LOCALAPPDATA%\Ankama\Dofus-dofus3` sous Windows, ou le dossier passé en argument) en lecture seule et réécrit `data/world.json` (~20 s). Commit, puis tag pour une nouvelle release.

## Tester

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test "tests/ui/*.test.js"
```

`cargo test -p dofus-data -- --ignored` relance en plus une extraction complète sur l'install locale.
