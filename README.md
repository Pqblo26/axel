<div align="center">

# Konofan Revival

Servidor local propio para volver a jugar a *KonoSuba: Fantastic Days* (Konofan), ya cerrado.

Uso personal · Windows + WSL2 + MuMu · basado en [Axel](https://github.com/Assasans/axel)

</div>

## Qué es

Un servidor que emula al oficial de Konofan y corre en mi propio PC, sin hosting. El juego (el cliente *stub* de Assasans) se conecta a `https://konofan.local` en lugar de a los servidores oficiales, y todo el progreso queda guardado en una base de datos local.

No es solo una copia para jugar: el objetivo es poder **controlar el juego desde el servidor**, es decir, decidir qué banners hay, con qué probabilidades, qué eventos están activos y cómo son los personajes, e incluso añadir contenido nuevo.

Parte de [Axel](https://github.com/Assasans/axel), el servidor de fans de Assasans, que ya resolvía lo más difícil (el protocolo, la firma de las respuestas y la protección LIAPP). Este fork lo adapta para jugar en Windows con MuMu y le va añadiendo los sistemas que faltaban.

## Estado

Se puede jugar desde el tutorial en adelante: misiones, historias, equipo, gacha, tienda y buzón funcionan sobre el servidor propio. El detalle de cada cambio, los fallos conocidos y las decisiones están en **[PROGRESO.md](PROGRESO.md)**.

**Funciona**

- Tutorial, cuenta y perfil; misiones principales, libres, de evento y de fama; historias (también las de colaboración); equipo y mejora de personajes; mazmorras; multijugador básico (heredado de Axel).
- **Gacha con las probabilidades reales** del juego original, garantía en la 10.ª tirada de una multi e historial de tiradas.
- **Monedas y consumos guardados**: gemas, eris y estamina (con recarga), tickets de salto, compras con límites y afinidad.
- Buzón de regalos, anuncios propios en *Note*, carrusel de la portada configurable, Jobs (expediciones) y Exchange.
- Recursos del juego (~35 GB) servidos desde una copia local, sin depender de ningún servidor externo.

**Falta**

- El gacha todavía no cobra gemas ni tickets, y los banners están escritos en el código.
- Rango y experiencia del jugador (la cuenta nace en rango 45 con todo desbloqueado, herencia de Axel).
- Medallas e intentos diarios de los bosses de evento y de Battle Arena; Fame Potions al empezar una Fame Quest.
- Combate: los enemigos no atacan en Auto y los golpes fallan sin Auto.
- El «+» de gemas y los productos de pago se cuelgan esperando a Google Play (solo se arregla en el cliente).
- Unos 100 métodos del protocolo siguen siendo *stubs*.

## Cómo está montado

```
MuMu (cliente stub)  --HTTPS-->  Windows :443  ==(WSL2, red en espejo)==>  nginx  -->  Axel :2020 / :2021  -->  PostgreSQL
```

- **Axel** (Rust, axum): la API del juego (`:2020`) y el servidor de recursos estáticos (`:2021`).
- **PostgreSQL**: jugadores, inventario, monedas, tiradas y probabilidades del gacha.
- **nginx**: pone el HTTPS (el juego no admite otra cosa) y junta todo bajo `konofan.local`.
- **WSL2 con Ubuntu 24.04**: Linux dentro de Windows, porque Axel y sus scripts están pensados para Linux.
- **MuMu** (Android 12): tiene instalada la CA propia como certificado de sistema y `10.0.2.2 konofan.local` en su `/etc/hosts`.

## Uso diario

En el escritorio de Windows:

| Acceso directo | Qué hace |
|---|---|
| `Konofan servidor.bat` | Arranca Axel dentro de WSL. |
| `Konofan registros.bat` | Muestra el registro del servidor en directo. |
| `Konofan apagar.bat` | Para Axel. |

En el launcher del stub, el servidor es `https://konofan.local/static/` (**con la barra final**) y el método de parcheo **LoadMetadataFile (And64InlineHook)**.

Para reportar un fallo basta con apuntar qué estabas haciendo y la hora aproximada: se cruza con los registros de `~/axel/logs`.

### Actualizar el servidor tras fusionar cambios

Dentro de Ubuntu (WSL):

```bash
cd ~/axel
git fetch origin
git checkout -B main origin/main
./migrations/migrate.sh
cargo build --release
```

Y después se reinicia Axel con `Konofan apagar.bat` y `Konofan servidor.bat`.

### Herramientas

- `scripts/konofan/enviar-regalo.sh`: manda gemas, eris, objetos o estamina al buzón de un jugador o de todos.
- `config/home-banners.toml`: qué banners salen en el carrusel de la portada.
- Tabla `news` de la base de datos: los anuncios de *Note*.

## Instalar desde cero

La guía paso a paso (WSL2, servidor, certificados, MuMu y cliente) está en **[docs/konofan/INSTALAR-WINDOWS-MUMU.md](docs/konofan/INSTALAR-WINDOWS-MUMU.md)**. El script `scripts/konofan/setup-wsl.sh` hace casi todo el trabajo dentro de Ubuntu, y `scripts/konofan/mirror-assets.sh` descarga la copia local de los recursos.

La documentación original de Axel, en inglés, sigue en [SERVER-SETUP.md](SERVER-SETUP.md), [DATABASE-SETUP.md](DATABASE-SETUP.md), [CLIENT-SETUP.md](CLIENT-SETUP.md) y [DUMPING.md](DUMPING.md).

## Hoja de ruta

1. ~~Redirigir el cliente al servidor local~~
2. ~~Login, carga del jugador y gacha con probabilidades reales~~
3. Sistemas principales: terminar monedas y consumos, combate, rango del jugador, eventos y Battle Arena.
4. Banners del gacha en la base de datos, para poder editarlos.
5. **Panel web de administración** en `https://konofan.local/admin/`: ver jugadores, tiradas y registros; editar banners, probabilidades, eventos y personajes.
6. Contenido nuevo: personajes, banners y eventos propios.

## Límites

- **Solo para uso personal y en local.** Konofan está cerrado; esto no es un servidor público ni se va a abrir a otros jugadores.
- **No se distribuyen el cliente ni los recursos originales** del juego. Este repositorio contiene solo código del servidor, scripts, configuración y los datos maestros que ya incluía Axel. Si algún día se comparte algo, serán herramientas o parches propios.
- La clave de firma (`key.pem`) es la pública de Axel; vale para uso local, pero habría que generar una propia si el servidor se expusiera fuera de casa.

## Créditos

- [Assasans](https://github.com/Assasans) por [Axel](https://github.com/Assasans/axel), el [cliente stub](https://github.com/Assasans/konofd-client-stub) y el espejo de recursos y APKs.
- CatzumaGaming por las tablas de probabilidades del gacha original.
- *KonoSuba: Fantastic Days* pertenece a sus respectivos titulares (Sumzap, Sesisoft y los de *KonoSuba*). Este proyecto no tiene relación con ellos.

## Licencia

Como Axel, el código está bajo licencia [MIT](LICENSE-MIT) o [Apache 2.0](LICENSE-APACHE), a elección.
