# NekoWall (prototipo, Fases 1 y parte de la 2)

Implementa del plan: núcleo, contrato `WallpaperBackend`, biblioteca SQLite,
backend Plasma para estáticos y CLI.

## Compilar y probar
    cargo test
    cargo build --release      # binario: target/release/nekowall

Requiere Rust ≥ 1.75. Dependencia externa: solo `rusqlite` (SQLite embebido).
No requiere systemd, X11 ni privilegios de administrador.

## Estado honesto
| Pieza | Estado |
| --- | --- |
| Modelos, contrato de backend, errores | Hecho, con tests |
| Biblioteca SQLite (importar, buscar, favoritos, faltantes) | Hecho, con tests |
| Servicio (apply, next/previous, restore) | Hecho, con tests (backend simulado) |
| Backend Plasma estático | Validado en Plasma 6.7.5 (`apply`, `restore`, `next`, `previous`, `status`) |
| Vídeo en Plasma (plugin Smart Video Wallpaper Reborn) | Escrito. El mecanismo (script por D-Bus) se validó a mano; **el código de NekoWall aún no se ha probado en el equipo** |
| `current()` con vídeo activo | Escrito (formato copiado de una configuración real); sin probar en el equipo |
| Miniaturas (`nekowall thumbs`, ffmpeg → `~/.cache/nekowall/thumbs`) | Hecho; 19 generadas en 2,9 s en el equipo |
| `list --json`, `status --json` | Hecho |
| HUD cinemático (`app/`, `nekowall-gui`) | Hecho y visto en el equipo (captura); navegación por teclado aún sin probar a mano |
| Vídeos más grandes que la pantalla: diálogo al importar (convertir a 1080p 30/60 fps o tal cual), «Optimizar para mi pantalla…», copia «nombre (1080p30).mp4» junto al original | Hecho; probado con copias y biblioteca de prueba (`NEKOWALL_DB`), sin clics reales |
| Pausa del vídeo (Ajustes → Vídeo: pantalla completa / ventana activa / cualquier ventana / nunca; batería) | Hecho; escritura verificada en Plasma con los mismos valores |
| Ventana principal (biblioteca, filtros, búsqueda, orden, favoritos, importar, quitar con deshacer, ajustes) | Hecho y visto en el equipo (capturas); interacción aún sin probar a mano |

## Validar en tu equipo (Void + Plasma 6.7.5)
    command -v plasma-apply-wallpaperimage   # ¿existe en tu instalación?
    nekowall import ~/Pictures/Wallpapers
    nekowall apply <id>                      # ¿cambia el fondo?
    nekowall apply <otro-id>; nekowall restore
    nekowall status                          # ¿detecta el fondo actual?

Puntos a verificar en el equipo real:
1. Que `plasma-apply-wallpaperimage` exista y funcione bajo KWin Wayland.
2. Que `~/.config/plasma-org.kde.plasma.desktop-appletsrc` refleje el fondo
   actual (`status`); Plasma puede escribirlo con retraso. Si no, hay que
   consultar por D-Bus.
3. Plugin de vídeo para Plasma 6 (Fase 4).

## Decisiones respecto al plan
- Los modelos viven en `nekowall-backend` (no hay `nekowall-core` de modelos);
  `nekowall-core` contiene la orquestación (`Service`).
- Se añadió `name()` al trait `WallpaperBackend`.
- CLI con parseo manual (sin clap) para minimizar dependencias.
- Puente GUI ↔ núcleo: la GUI (Qt/QML, C++) llama a la CLI con `--json` mediante QProcess asíncrono,
  en lugar de CXX-Qt. Compila en segundos, sin crates nuevos, y la UI no se bloquea. Se puede migrar
  a CXX-Qt sin tocar el QML si el coste de lanzar procesos llega a notarse.
- Licencia provisional GPL-3.0-or-later; decídela antes de reutilizar código.

## Vídeo en Plasma (lo aprendido en el equipo real)
- Plugin: `luisbocanegra.smart.video.wallpaper.reborn` (v2.17.0 compilado desde fuente en Void).
- Se activa enviando un script a `org.kde.plasmashell /PlasmaShell evaluateScript` con `gdbus`.
  Si `plasmashell` está colgado, `/PlasmaShell` no existe y NekoWall informa
  «plasmashell no responde por D-Bus» en lugar de un error vacío.
- Plasma conserva la configuración de plugins inactivos (p. ej. `Image=`), por eso
  `current()` se guía por `wallpaperplugin=` y no por la última clave vista.

## Pruebas pendientes en el equipo
    nekowall apply <id-de-un-.mp4>    # ¿cambia a vídeo?
    nekowall status                   # ¿muestra el vídeo y vídeo=true?
    nekowall apply <id-de-un-.png>    # ¿vuelve a imagen y se detiene el vídeo?
    nekowall next / previous          # ahora recorre también los vídeos

## Interfaz gráfica (app/)
    cargo build --release                      # la GUI usa target/release/nekowall
    cmake -S app -B app/build -DCMAKE_BUILD_TYPE=Release && cmake --build app/build
    app/build/nekowall-gui                     # ventana principal
    app/build/nekowall-gui --hud               # selector rápido (HUD)

HUD: ←/→ o rueda = navegar · Enter o clic en la tarjeta central = aplicar · F = favorito ·
Esc o clic fuera = cerrar. Usa `org.kde.layershell` (paquete `layer-shell-qt`) para dibujarse
encima de todo en Wayland. Si en KDE la velocidad de animación está en 0 (`kdeglobals`
`AnimationDurationFactor=0`) o `NEKOWALL_REDUCED_MOTION=1`, quita inclinación y animaciones.
`NEKOWALL_TIMING=1` mide el primer fotograma (medido: HUD ~262 ms, ventana principal ~584 ms desde `main`;
la meta del HUD es < 150 ms).

Ventana principal: Ctrl+F buscar · Ctrl+O añadir archivos · Ctrl+Mayús+O añadir carpeta · Ctrl+H HUD ·
F5 recargar · Ctrl+Z deshacer «quitar» · en la rejilla: Enter aplicar, F favorito, Supr quitar.
También se puede arrastrar archivos o carpetas a la ventana.

Lanzador: `~/.local/share/applications/nekowall.desktop` (menú = ventana principal; acción
«Selector rápido» = `--hud` con Meta+Alt+W, registrada en kglobalaccel).

## Rendimiento (medido el 2026-11-14 en el T480s)
- NekoWall cerrado: 0 MiB. HUD ~65 MiB, ventana ~77 MiB (PSS). Cada llamada a la CLI: 7-8 ms.
- Vídeo en Plasma: plasmashell 16-23 % de CPU reproduciendo, 3,5 % en pausa (ventana a pantalla completa).
  Decodificar por VA-API cuesta ~3 %: el resto es la tubería de Qt Multimedia del plugin.
- Conversión 4K→1080p (14 s de vídeo): libx264 veryfast 9,6 s y 3,7 MB (SSIM 0,987). Por VA-API
  `scale_vaapi` falla al redimensionar (iHD + ffmpeg 6.1) y el camino híbrido es más lento (14-15 s, 7-12 MB),
  así que se convierte con el procesador a prioridad baja (`nice -n 10`).

CLI nueva: `nekowall probe <ruta>...`, `nekowall optimize <id|ruta> [--height N] [--fps N] [--replace-in-library]`,
`nekowall video-settings [--pause-mode 0-3] [--battery on|off] [--battery-level N] [--json]`,
`nekowall relocate <carpeta-vieja> <carpeta-nueva>` (tras mover una carpeta: rutas, favoritos y miniaturas).
GUI: `nekowall-gui --import <ruta>...`. Ajustes de la GUI en `~/.config/nekowall/nekowall.conf`.

## Estilo del HUD (Dark Aero, 2026-11-14)
Ventana transparente con desenfoque de KWin (`KWindowEffects::enableBlurBehind`, opcional vía KF6WindowSystem),
paneles de cristal con brillo, acento turquesa Miku (#39C5BB), halo de neón (`RectangularShadow`) en la tarjeta
activa, sin reflejos. Ilustración opcional a la derecha con máscara alfa (`Qt5Compat.GraphicalEffects.OpacityMask`):
la elige el usuario en Ajustes y se copia a `~/.local/share/nekowall/mascota.*` (no se distribuye con el programa).
Icono: tema de usuario `~/.local/share/icons/hicolor/*/apps/nekowall.png` (tampoco está en el repositorio).

## Re-evaluación de rendimiento (2026-11-14, en batería, pantalla encendida)
- Fondo: estática 0,6 % CPU / 361 MiB plasmashell · Cherry Pop 4K 25,8 % / 497 MiB · 1080p30 22,0 % / 397 MiB ·
  1080p30 en pausa (ventana activa) 3,1 %. Consumo del portátil 9,4-10,1 W en todos (dentro del ruido).
- HUD Dark Aero optimizado: 90 → 61-63 MiB PSS con el mismo aspecto. La ilustración se «hornea» una vez
  (`Bridge::bakedMascot`, ~/.cache/nekowall/hud-art-*.png, recortada y fundida); el fondo de la selección es la
  miniatura a 48 px ampliada (sin MultiEffect); las tarjetas decodifican a 420 px. Ya no usa Qt5Compat.
- Probado sin ganancia útil: estilo Fusion (-2 MiB), MALLOC_ARENA_MAX=2 y QSG_RENDER_LOOP=basic (-1 a -3 MiB).
- Tiempo de apertura con el gobernador `powersave` (batería): 280-800 ms, muy variable; la ventana principal, sin
  cambios, también subió. Pendiente medir enchufado para comparar con los ~200 ms de la noche anterior.
