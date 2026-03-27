# Milestone 6: "El sandbox completo" ✅ COMPLETO (2026-03-27)

**Objetivo:** Sandbox con gestión de escenas, salto, configuración de entorno, props desde archivo externo.

**Resultado:** Salto con raycast grounded + coyote time + sonidos procedurales. Menú principal (Continuar/Nueva/Cargar/Salir) con auto-save en `.last_session.json`. Pantalla de configuración con dropdowns escaneados de carpetas (skyboxes, terrains, characters, props). Props catalog desde `default_props.json` (14 props con scale individual), panel con filtro por categoría y búsqueda, placement con raycast al plano y=0, grid snap (G toggle, 0.5/1.0/2.0), delete/duplicar/undo. Config persistente en `config.json` (MSAA, SSAO, tone mapping, LOD, bloom, volumen). Fix de brillo IBL via `ibl_scale` en `dirLightDir.w`.

**Decisiones tomadas:**
- `is_grounded()` via raycast hacia abajo, `max_toi = 1.05`
- Salto = impulso vertical en rapier body (`apply_jump_impulse`)
- Coyote time 0.1s
- Sonidos jump/land/footstep generados proceduralmente
- AppScreen enum: MainMenu / Settings / InScene
- Auto-save en `scenes/.last_session.json` al cerrar o salir a menú
- Scan de carpetas con `scan_dir_files()` para listar opciones de config
- Props con `scale: [x,y,z]` individual en el catálogo
- PlacedPropTag en ECS para serialización de objetos colocados
- IBL placeholder blanco causaba brillo excesivo → `ibl_scale` default 0.2
- Bloom desacoplado de hardcode, controlable en sidebar
- `config.json` guarda/carga settings globales al arrancar/cerrar

| Fase | Descripción | Estado |
|------|-------------|--------|
| 33 | Salto (raycast grounded, impulso rapier, coyote time) | ✅ |
| 34 | Menú principal + gestión de escenas + auto-save | ✅ |
| 35 | Pantalla configuración (skybox, terreno, character, props) | ✅ |
| 36 | Props catalog JSON + placement + grid snap + undo | ✅ |
| 37 | Assets, audio, polish, empaquetado | ✅ |

**Siguiente:** → `docs/milestone-7.md`