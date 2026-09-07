# nu_plugin_junit

Plugin **nushell natif** (protocole `nu_plugin_*`, écrit en Rust) qui parse
des rapports JUnit/surefire et produit un rapport Excel stylé — directement
dans dans le terminal `nushell`.

## Commandes

### `junit report [path]`

Parse tous les `*.xml` d'un répertoire de rapports surefire et retourne une
table `{ suite, test, status, time }` (`time` en secondes). L'attribut
`name` du `<testsuite>` (complété par `classname` du `<testcase>` pour le
troisième cas) peut suivre plusieurs conventions selon le générateur du
rapport, détectées automatiquement :

- **Style Java classique** (Maven/Surefire) — `name` est un nom pleinement
  qualifié à points (ex: `org.outil.SomeTest`) : `suite` = dernier segment
  (`SomeTest`).
- **Style "fichier de test"** (Jest, Mocha, pytest, et d'autres générateurs
  non-Java) — `name` est directement un chemin de fichier (ex:
  `tests\integration\mon_test.test.js`, détecté par la présence d'un `/` ou `\`) :
  `suite` = nom de fichier complet, extension incluse (`mon_test.test.js`).
- **Style Karma/Jasmine** (Angular + `karma-junit-reporter`, config par
  défaut) — `name` est l'identifiant du navigateur (ex: `Chrome Headless
  152.0.0.0 (Windows 10)`), identique pour tous les tests du run donc
  inutilisable tel quel. Le vrai regroupement (bloc `describe` Jasmine) est
  reconstruit depuis l'attribut `classname` de chaque `<testcase>`, qui le
  préfixe par ce même nom de navigateur "assaini" (espaces et points
  remplacés par `_`) — comportement par défaut de `karma-junit-reporter`,
  non configurable côté Karma (l'attribut `name` du `<testsuite>` y est
  câblé en dur sur le nom du navigateur, sans option pour le changer).
  `suite` = bloc `describe` (ex: `TypologyTableItemComponent`).

(Une colonne `file` séparée existait dans une version précédente, mais
faisait doublon avec `suite` dans la quasi-totalité des cas réels — elle a
été retirée, `suite` porte maintenant directement l'info la plus utile
selon le format du rapport.)

Les rapports où plusieurs `<testsuite>` sont regroupés sous un `<testsuites>`
englobant (courant avec Jest/jest-junit et d'autres générateurs non-Java)
sont gérés correctement : chaque `<testsuite>` est traité individuellement,
jamais le nom du `<testsuites>` englobant.

```nu
junit report ./target/surefire-reports
# ou, avec le défaut (target/surefire-reports relatif au dossier courant) :
junit report
```

Comme c'est une table nushell normale, tout le reste de nushell s'applique
sans rien écrire de plus :

```nu
junit report | where status == "failed"
junit report | group-by status | transpose status cases | insert count {|r| $r.cases | length}
junit report | sort-by time --reverse | first 5
junit report | to csv | save resultats.csv
junit report | to json | save resultats.json
```

### `junit to-xlsx <path> [--project <nom>] [--branch <branche>] [--open]`

Consomme une table (typiquement la sortie de `junit report`, éventuellement
filtrée/triée avant) et écrit un rapport Excel `.xlsx` stylé : classes de
test fusionnées (colonne "Classe de test" = `suite`), statuts colorés,
formules `COUNTIF` pour les totaux, durée totale, pied de page
projet/branche.

`--open` (ou `-o`) ouvre le fichier généré avec l'application par défaut
associée aux `.xlsx` (Excel, LibreOffice Calc...), via `start` sous Windows,
`open` sous macOS, `xdg-open` sous Linux. C'est **opt-in** (jamais par
défaut — gênant en CI/scripté) et **non bloquant** : si l'ouverture échoue
(pas d'appli associée, environnement headless...), la commande n'échoue pas
pour autant — le fichier a bien été généré, c'est signalé dans le message
retourné :

```nu
junit report ./target/surefire-reports
  | junit to-xlsx ./rapport.xlsx --project mon-projet --branch main --open
# => Rapport écrit : /chemin/absolu/vers/rapport.xlsx
```

## Build

**Important : la version de nu-plugin/nu-protocol doit correspondre exactement à ta version de nushell.** Le protocole de plugin nushell refuse de charger un plugin compilé contre une autre version — pas de compatibilité ascendante/descendante. Avant de builder, vérifie ta version :

```nu
version | get version
```

Puis dans `Cargo.toml`, aligne `nu-plugin`, `nu-protocol` et
`nu-plugin-test-support` (dev-dependency) sur exactement cette version
(les trois doivent rester synchronisées entre elles). Au moment de la
rédaction, ce projet cible **nushell 0.114.1** — si ta version diffère :

```bash
cargo add nu-plugin@<ta_version> --no-default-features
cargo add nu-protocol@<ta_version>
cargo add nu-plugin-test-support@<ta_version> --dev
```

(`--no-default-features` reproduit le `default-features = false` déjà en
place sur `nu-plugin` dans le `Cargo.toml` fourni — voir plus bas
pourquoi.) Si nushell se met à jour plus tard (ce qui arrive souvent),
il faudra refaire cette opération et recompiler.

```bash
cargo build --release
# binaire : target/release/nu_plugin_junit
```

## Installation dans nushell

```nu
plugin add ./target/release/nu_plugin_junit
plugin use junit
```

Ensuite `junit report` et `junit to-xlsx` sont disponibles dans toutes tes
sessions nushell (l'enregistrement est persistant, pas besoin de refaire
`plugin add` à chaque lancement).

## Tests

```bash
cargo test
```

Les tests utilisent `nu-plugin-test-support` (`PluginTest`), qui fait
tourner un vrai moteur/parseur nushell **en process**, sans passer par le
protocole stdio ni un binaire `nu` externe — c'est le moyen recommandé de
tester un plugin nushell. Trois tests fournis :
- parsing d'un répertoire de rapports en table,
- gestion d'erreur sur un répertoire inexistant,
- pipeline complet `junit report | junit to-xlsx` avec vérification du
  fichier `.xlsx` produit (archive OOXML valide, cellules et fusions
  correctes — vérifié manuellement avec `openpyxl` pendant le
  développement).


## Publier une release (build multi-plateformes)

Le workflow fourni dans `.github/workflows/release.yml` construit le
plugin pour 4 cibles (Linux x86_64/aarch64, macOS Intel/Apple Silicon,
Windows x86_64) et publie chaque binaire en asset d'une release GitHub dès
qu'un tag `vX.Y.Z` est poussé :

```bash
git tag v0.1.0
git push origin v0.1.0
```

Chaque asset est nommé `nu_plugin_junit-<target-triple>.(tar.gz|zip)`,
ex. `nu_plugin_junit-x86_64-unknown-linux-gnu.tar.gz` — c'est la convention
de nommage que le backend `github` de mise sait reconnaître automatiquement
pour choisir le bon binaire selon l'OS/l'architecture de la machine.

La cible `aarch64-unknown-linux-gnu` est cross-compilée via
[`cross`](https://github.com/cross-rs/cross) (pas de runner GitHub natif
pour cette architecture) ; les autres sont compilées nativement sur leur
plateforme. Comme le projet n'a aucune dépendance C système (voir la note
de compilation plus haut), le cross-build ne demande aucune configuration
supplémentaire.

## Intégration dans mise

```toml
[tools]
"github:nsenoge63/nu_plugin_junit" = "<version-plugin>"
```

```bash
mise use "github:nsenoge63/nu_plugin_junit@<version-plugin>"
```

mise télécharge et installe le bon binaire pour la plateforme courante et
l'expose sur le PATH. Il reste ensuite une seule étape, à faire une fois,
**dans nushell** (mise ne peut pas le faire à ta place : c'est nushell qui
gère la liste de ses plugins enregistrés, indépendamment de mise) :

```nu
plugin add (which nu_plugin_junit | get path.0)
plugin use junit
```

Cet enregistrement est persistant d'une session nushell à l'autre — pas
besoin de le refaire à chaque ouverture de terminal, seulement après une
mise à jour du binaire.


