/* ═══════════════════════════════════════════════════════════════════════
   Transición de cuadraditos · portada de web/index.html (startCanvas/sampleGrad)
   Cortina de revelado: una retícula de cuadrados oscuros cubre la página y se
   disuelve en onda radial invertida (los bordes primero, converge hacia el
   foco donde vive la esfera), con un flash por celda muestreado del degradado
   de marca #2FD9FF → #8B5CF6 (55%) → #F23DC4.
   Sin dependencias: canvas 2D puro. window.CUADRADITOS.revelar({...}).
   ═══════════════════════════════════════════════════════════════════════ */
(function () {
  "use strict";

  /* degradado de marca, interpolación RGB lineal */
  function sampleGrad(f) {
    f = Math.max(0, Math.min(1, f));
    const st = [
      [0, 47, 217, 255], // #2FD9FF
      [0.55, 139, 92, 246], // #8B5CF6
      [1, 242, 61, 196], // #F23DC4
    ];
    let i = 0;
    while (i < st.length - 1 && f > st[i + 1][0]) i++;
    const a = st[i],
      b = st[Math.min(i + 1, st.length - 1)];
    const t = (f - a[0]) / (b[0] - a[0] || 1);
    const r = Math.round(a[1] + (b[1] - a[1]) * t);
    const g = Math.round(a[2] + (b[2] - a[2]) * t);
    const bl = Math.round(a[3] + (b[3] - a[3]) * t);
    return "rgb(" + r + "," + g + "," + bl + ")";
  }

  /* opts: { canvas, cortina?, focoX?=0.5, focoY?=0.42, celda?, alFinal? } */
  function revelar(opts) {
    const cv = opts.canvas;
    if (!cv) {
      if (opts.cortina) opts.cortina.remove();
      if (opts.alFinal) opts.alFinal();
      return;
    }
    const ctx = cv.getContext("2d");
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const W = window.innerWidth,
      H = window.innerHeight;
    cv.width = Math.round(W * dpr);
    cv.height = Math.round(H * dpr);
    /* el tamaño CSS lo da inset:0 — si la ventana cambia durante los ~2 s de
       animación el canvas se estira y nunca queda una franja sin cubrir */
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const cell = opts.celda || (W < 640 ? 46 : 40);
    const cols = Math.ceil(W / cell),
      rows = Math.ceil(H / cell);
    const cx = W * (opts.focoX == null ? 0.5 : opts.focoX);
    const cy = H * (opts.focoY == null ? 0.42 : opts.focoY);
    const cells = [];
    let maxD = 1;
    for (let r = 0; r < rows; r++)
      for (let c = 0; c < cols; c++) {
        const x = c * cell,
          y = r * cell,
          mx = x + cell / 2,
          my = y + cell / 2;
        const d = Math.hypot(mx - cx, my - cy);
        if (d > maxD) maxD = d;
        cells.push({ x, y, mx, my, d });
      }
    const SPREAD = 1450,
      DUR = 520;
    for (const k of cells) {
      /* bordes primero (onda que converge al foco) + jitter determinista */
      k.delay =
        SPREAD * (1 - k.d / maxD) + (((k.mx * 13 + k.my * 29) % 97) / 97) * 80;
      k.col = sampleGrad((k.mx / W + k.my / H) / 2);
    }

    function pintar(t) {
      ctx.clearRect(0, 0, W, H);
      for (const k of cells) {
        const lt = t - k.delay;
        let baseA = 0,
          hotA = 0;
        if (lt < 0) {
          baseA = 1;
        } else if (lt < DUR) {
          const u = lt / DUR;
          baseA = u < 0.44 ? 1 : 1 - (u - 0.44) / 0.56;
          hotA = u < 0.44 ? u / 0.44 : Math.max(0, 1 - (u - 0.44) / 0.42);
        }
        if (baseA <= 0 && hotA <= 0) continue;
        if (baseA > 0) {
          ctx.globalAlpha = baseA;
          ctx.fillStyle = "#05060F";
          ctx.fillRect(k.x, k.y, cell + 1, cell + 1);
        }
        if (hotA > 0) {
          ctx.globalAlpha = hotA * 0.95;
          ctx.fillStyle = k.col;
          ctx.fillRect(k.x + 1.5, k.y + 1.5, cell - 3, cell - 3);
        }
      }
      ctx.globalAlpha = 1;
    }

    /* primer cuadro síncrono con todo cubierto; recién entonces cae la cortina
       sólida — así nunca se ve la página "pelada" antes de la onda.
       Mientras dura la onda, el canvas intercepta los clics: nada de pulsar
       enlaces todavía tapados por cuadros opacos. */
    cv.style.pointerEvents = "auto";
    pintar(-1);
    if (opts.cortina) opts.cortina.remove();

    const start = performance.now();
    let raf;
    function draw(now) {
      const t = now - start;
      pintar(t);
      if (t < SPREAD + DUR + 80) {
        raf = requestAnimationFrame(draw);
      } else {
        ctx.clearRect(0, 0, W, H);
        cv.remove();
        if (opts.alFinal) opts.alFinal();
      }
    }
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }

  window.CUADRADITOS = { revelar, sampleGrad };
})();
