
## [0.3.0] - 2026-09-07

### Added

- Prise en compte des fichiers générés par `karma`
- Commande `junit to-xlsx` : option `-o/--open` pour ouvrir le fichier généré avec l'application par défaut associée aux `.xlsx` (Excel, LibreOffice Calc...)

---

## [0.2.0] - 2026-08-14

### Changed

- Récupération du nom du fichier dans le champ `suite` pour les fichiers de tests non-Java (Jest, Mocha, pytest, etc.)
- Alignement à gauche des cellules plutôt que centrées
- Revue du README

---

## [0.1.0] - 2026-08-11

Version initiale du plugin

### Added

- Commande `junit report` : Parse tous les `*.xml` d'un répertoire de rapports surefire et retourne une table `{ suite, test, status, time }` (`time` en secondes).
- Commande `junit to-xlsx` : Consomme une table (typiquement la sortie de `junit report`, éventuellement filtrée/triée avant) et écrit un rapport Excel `.xlsx`
