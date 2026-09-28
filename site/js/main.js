// ============================================================
// HyperScanEmu — Landing Page Scripts
// i18n + Compatibility Gallery + Animations
// ============================================================

(function () {
  'use strict';

  // ================================================================
  // GAME DATA — from docs/Game-Compatibility.md (local boot batch)
  // ================================================================
  var GAMES = [
    {
      status: 'pass',
      zh: '少年骇客',
      en: 'Ben 10',
      img: 'docs/images/Ben_10__USA___USE1_.png',
      noteZh: '640×240 游戏加载画面',
      noteEn: '640×240 game loading screen'
    },
    {
      status: 'pass',
      zh: '漫威英雄',
      en: 'Marvel Heroes',
      img: 'docs/images/Marvel_Heroes__USA___USE2_.png',
      noteZh: '角色立绘 + 进度条',
      noteEn: 'Character art + progress bar'
    },
    {
      status: 'pass',
      zh: '蜘蛛侠',
      en: 'Spider-Man',
      img: 'docs/images/Spider-Man__USA_.png',
      noteZh: '320×240 游戏加载画面',
      noteEn: '320×240 game loading screen'
    },
    {
      status: 'pass',
      zh: 'X战警',
      en: 'X-Men',
      img: 'docs/images/X-Men__USA___USE_.png',
      noteZh: '640×480 游戏加载画面',
      noteEn: '640×480 game loading screen'
    },
    {
      status: 'partial',
      zh: '星际摔角联盟',
      en: 'Interstellar Wrestling League',
      img: 'docs/images/IWL_-_Interstellar_Wrestling_League__USA___USE1_.png',
      noteZh: '系统加载中，暂无游戏专属画面',
      noteEn: 'System loading; no game-specific frame yet'
    }
  ];

  // ================================================================
  // i18n — Translations
  // ================================================================
  var I18N = {
    zh: {
      'nav-features': '核心特性',
      'nav-games': '兼容进度',
      'nav-arch': '技术架构',
      'nav-quickstart': '快速开始',
      'hero-subtitle': '让 2006 年的 Mattel HyperScan 在现代设备上重生',
      'hero-desc': '用 Rust 编写的 SPG290 低级仿真（LLE）模拟器，启动真实固件与零售光盘镜像，抵达游戏加载画面',
      'hero-download': '下载',
      'hero-github': '查看源码',
      'hero-shot-note': 'HyperScanEmu 实际启动画面 · X-Men 零售镜像',
      'hero-scroll': '向下滚动探索',
      'about-title': '何为 HyperScan？',
      'about-p1': 'Mattel HyperScan 是 2006 年发售的家用游戏主机，核心是 <strong>Sunplus SPG290</strong> SoC（S+Core 7 CPU）。游戏以光盘发行，并可选配 RFID 存档卡。',
      'about-p2': '这台主机生命周期短暂、资料稀缺。HyperScanEmu 选择<strong>固件可见硬件的低级仿真</strong>路线：不猜测私有服务 ABI，而是逐寄存器还原 SPG290 总线、外设与定时行为，让真实 BIOS 与游戏镜像自己跑起来。',
      'flow-firmware': '合法固件 Dump',
      'flow-media': 'BIN / CUE / ZIP',
      'flow-play': '启动验证',
      'spec-title': '硬件要点',
      'spec-cpu': 'CPU',
      'spec-soc': 'SoC',
      'spec-video': '视频',
      'spec-audio': '音频',
      'spec-media': '介质',
      'spec-card': '存档',
      'stat-packages': '已测镜像包',
      'stat-packages-sub': '合法本地 Dump',
      'stat-pass': '抵达加载画面',
      'stat-pass-sub': '游戏专属 Loading',
      'stat-res': '输出分辨率',
      'stat-res-sub': 'TVE 直帧 + PPU 图层',
      'stat-trace': '参考启动指令数',
      'stat-trace-sub': '无未知 MMIO 中止',
      'feat-title': '核心特性',
      'feat-subtitle': '从 S+Core 7 解释器到 CD 伺服，全链路 Rust 低级仿真',
      'feat-cpu-title': 'SPG290 LLE 核心',
      'feat-cpu-desc': 'S+Core 7 混合 16/32 位指令流解释器，校验式 MMIO 总线，确定性外设调度。未知指令与未知 MMIO 一律结构化中止，绝不静默成功。',
      'feat-fw-title': '真实固件启动',
      'feat-fw-desc': '32 KiB 内部 ROM + 1 MiB HyperScan BIOS。冷启动走外部 BIOS 窗口，参考运行可完成 1500 万条指令且无未知 MMIO 中止。',
      'feat-disc-title': '光盘镜像与 CD 伺服',
      'feat-disc-desc': '支持 MODE1/2352 BIN、单轨 CUE 与 ZIP，含 ISO/UDF 校验。定时扇区 DMA、模拟反馈与单轨 Q 子通道合成，让固件完成聚焦/循迹标定。',
      'feat-video-title': 'TVE + PPU 显示',
      'feat-video-desc': 'TVE 直帧路径与 PPU 位图/字符/精灵图层，支持调色板、行表、透明、混合与淡入淡出，输出 640×480 画面。',
      'feat-input-title': '双手柄 I²C 与 RFID',
      'feat-input-desc': '周期调度的 I²C 主机暴露双控制器（按键、模拟摇杆、校验和）；GPIO 上实现 120 字节 RFID 卡协议，含 REQA/WUPA/RID/READ/WRITE。',
      'feat-audio-title': 'DAC 音频通路',
      'feat-audio-desc': '54 MHz DAC 环形缓冲 DMA 输出无符号 16-bit 立体声 PCM，带半缓冲中断。硬件合成器（PCM/ADPCM 通道）仍在路线图上。',
      'feat-frontend-title': 'Standalone 前端',
      'feat-frontend-desc': '可缩放窗口、全屏、双人键盘映射、PNG 截图。亦可完全 headless 运行，用统一生命周期做回归与帧指纹。',
      'feat-libretro-title': 'libretro 核心',
      'feat-libretro-desc': '完整 RetroArch 集成，固件从 frontend system 目录加载。Windows / macOS / Linux / Android / iOS / webOS 多平台目标。',
      'gallery-title': '兼容进度',
      'gallery-subtitle': '8 个本地镜像包的 headless 启动验证 · 记录启动里程碑，而非完整游玩认证',
      'ms-300': '开机画面',
      'ms-900': 'CD 识别 / TOC',
      'ms-1600': '游戏加载画面',
      'ms-3600': '开场动画',
      'gallery-note': '固件与游戏数据从不随模拟器分发；以上结果仅来自合法转储的本地镜像。',
      'gallery-more': '查看完整兼容性列表 →',
      'status-pass': '加载画面',
      'status-partial': '部分',
      'arch-title': '技术架构',
      'arch-subtitle': '平台无关的核心引擎，薄前端适配层，边界清晰',
      'arch-frontends': '前端适配',
      'arch-standalone-sub': 'CLI / 窗口 / 音频<br>headless 驱动',
      'arch-libretro-sub': 'libretro C ABI<br>RetroArch 核心',
      'arch-media': '介质层',
      'arch-media-sub': '严格 BIN / CUE / ZIP 解析<br>ISO/UDF 校验 · 路径安全',
      'arch-core': '核心引擎',
      'arch-core-sub': '平台无关的客体可见状态',
      'principle-1-title': '证据驱动',
      'principle-1-desc': '行为以固件痕迹、寄存器手册与单元测试锁定，而不是猜测。',
      'principle-2-title': '确定性',
      'principle-2-desc': '外设时间由模拟 CPU 周期推进，不依赖主机墙钟，可复现回归。',
      'principle-3-title': '结构化失败',
      'principle-3-desc': '未知指令 / MMIO 带 PC 与地址精确中止，方便诊断而非静默吞掉。',
      'qs-title': '快速开始',
      'qs-subtitle': '需要自备合法固件与游戏镜像 · 仓库从不分发受版权保护的数据',
      'qs-standalone': 'Standalone',
      'qs-standalone-1': '编译或下载发布版',
      'qs-standalone-2': '准备固件与镜像',
      'qs-standalone-3': '启动游戏',
      'qs-retro-1': '安装 libretro 核心',
      'qs-retro-1-sub': '重命名为 hyperscanemu_libretro.&lt;ext&gt;',
      'qs-retro-2': '放置固件到 system 目录',
      'qs-retro-3': '加载核心与内容',
      'qs-diag': '诊断 CLI',
      'qs-diag-1': '校验镜像包',
      'qs-diag-2': '指令级追踪',
      'qs-diag-3': '无头帧捕获',
      'footer-desc': '用 Rust 编写的 Mattel HyperScan 模拟器 · SPG290 低级仿真',
      'footer-project': '项目',
      'footer-docs': '文档',
      'footer-arch': '架构',
      'footer-hw': '硬件',
      'footer-compat': '兼容性',
      'footer-contributing': '贡献指南',
      'footer-platforms': '平台',
      'footer-copy': 'BSD 3-Clause License © 2025–2026 Aloys. Built with Rust.'
    },
    en: {
      'nav-features': 'Features',
      'nav-games': 'Compatibility',
      'nav-arch': 'Architecture',
      'nav-quickstart': 'Quick Start',
      'hero-subtitle': 'Revive the 2006 Mattel HyperScan on modern hardware',
      'hero-desc': 'A Rust SPG290 low-level emulator that boots real firmware and retail disc images through to game loading screens',
      'hero-download': 'Download',
      'hero-github': 'Source',
      'hero-shot-note': 'HyperScanEmu actual boot frame · X-Men retail image',
      'hero-scroll': 'Scroll to explore',
      'about-title': 'What is HyperScan?',
      'about-p1': 'The Mattel HyperScan is a 2006 home console built around the <strong>Sunplus SPG290</strong> SoC (S+Core 7 CPU). Games ship on CD with optional RFID save cards.',
      'about-p2': 'The platform was short-lived and poorly documented. HyperScanEmu pursues <strong>firmware-visible low-level emulation</strong>: no guessed private service ABI — just register-accurate SPG290 buses, peripherals and timing so real BIOS and retail images run themselves.',
      'flow-firmware': 'Legal firmware dump',
      'flow-media': 'BIN / CUE / ZIP',
      'flow-play': 'Boot validation',
      'spec-title': 'Hardware highlights',
      'spec-cpu': 'CPU',
      'spec-soc': 'SoC',
      'spec-video': 'Video',
      'spec-audio': 'Audio',
      'spec-media': 'Media',
      'spec-card': 'Saves',
      'stat-packages': 'Packages tested',
      'stat-packages-sub': 'Legal local dumps',
      'stat-pass': 'Reach loading screen',
      'stat-pass-sub': 'Game-specific loading',
      'stat-res': 'Output resolution',
      'stat-res-sub': 'TVE direct + PPU layers',
      'stat-trace': 'Reference boot instructions',
      'stat-trace-sub': 'No unknown MMIO stop',
      'feat-title': 'Features',
      'feat-subtitle': 'Rust low-level emulation from S+Core 7 to CD servo',
      'feat-cpu-title': 'SPG290 LLE core',
      'feat-cpu-desc': 'S+Core 7 mixed 16/32-bit interpreter, checked MMIO bus, deterministic peripheral scheduling. Unknown instructions and MMIO stop with structured diagnostics — never silent success.',
      'feat-fw-title': 'Real firmware boot',
      'feat-fw-desc': '32 KiB internal ROM plus 1 MiB HyperScan BIOS. Cold reset enters the external BIOS window; a reference run completes 15M instructions with no unknown MMIO stop.',
      'feat-disc-title': 'Disc images & CD servo',
      'feat-disc-desc': 'MODE1/2352 BIN, single-track CUE and ZIP with ISO/UDF validation. Timed sector DMA, analog feedback and Q-subchannel synthesis complete focus/tracking calibration.',
      'feat-video-title': 'TVE + PPU display',
      'feat-video-desc': 'TVE direct frames plus PPU bitmap, character and sprite layers with palettes, line tables, transparency, blending and fade — 640×480 output.',
      'feat-input-title': 'Dual I²C pads & RFID',
      'feat-input-desc': 'Cycle-scheduled I²C master exposes two controllers (buttons, analog axes, checksums). GPIO implements the 120-byte RFID card protocol with REQA/WUPA/RID/READ/WRITE.',
      'feat-audio-title': 'DAC audio path',
      'feat-audio-desc': '54 MHz DAC ring-buffer DMA emits unsigned 16-bit stereo PCM with half-buffer IRQ. Hardware synthesizer (PCM/ADPCM channels) remains on the roadmap.',
      'feat-frontend-title': 'Standalone frontend',
      'feat-frontend-desc': 'Resizable window, fullscreen, dual keyboard gamepads, PNG screenshots. Fully headless runs share the same lifecycle for regression and frame fingerprints.',
      'feat-libretro-title': 'libretro core',
      'feat-libretro-desc': 'Full RetroArch integration with firmware from the frontend system directory. Targets Windows, macOS, Linux, Android, iOS and webOS.',
      'gallery-title': 'Compatibility',
      'gallery-subtitle': 'Headless boot validation of 8 local packages · boot milestones, not full gameplay certification',
      'ms-300': 'Startup frame',
      'ms-900': 'CD identify / TOC',
      'ms-1600': 'Game loading screen',
      'ms-3600': 'Opening animation',
      'gallery-note': 'Firmware and game data are never shipped with the emulator; results come only from legally dumped local images.',
      'gallery-more': 'See the full compatibility list →',
      'status-pass': 'Loading screen',
      'status-partial': 'Partial',
      'arch-title': 'Architecture',
      'arch-subtitle': 'Platform-independent core with thin host adapters',
      'arch-frontends': 'Frontends',
      'arch-standalone-sub': 'CLI / window / audio<br>headless driver',
      'arch-libretro-sub': 'libretro C ABI<br>RetroArch core',
      'arch-media': 'Media layer',
      'arch-media-sub': 'Strict BIN / CUE / ZIP parsing<br>ISO/UDF checks · safe paths',
      'arch-core': 'Core engine',
      'arch-core-sub': 'Platform-independent guest-visible state',
      'principle-1-title': 'Evidence-driven',
      'principle-1-desc': 'Behavior is locked by firmware traces, manuals and unit tests — not guesses.',
      'principle-2-title': 'Deterministic',
      'principle-2-desc': 'Peripheral time advances from emulated CPU cycles, not host wall-clock.',
      'principle-3-title': 'Structured failure',
      'principle-3-desc': 'Unknown instruction / MMIO stops with exact PC and address for diagnosis.',
      'qs-title': 'Quick Start',
      'qs-subtitle': 'Supply your own legally obtained firmware and game images · never redistributed here',
      'qs-standalone': 'Standalone',
      'qs-standalone-1': 'Build or download a release',
      'qs-standalone-2': 'Prepare firmware and media',
      'qs-standalone-3': 'Launch the game',
      'qs-retro-1': 'Install the libretro core',
      'qs-retro-1-sub': 'Rename to hyperscanemu_libretro.&lt;ext&gt;',
      'qs-retro-2': 'Place firmware in system/',
      'qs-retro-3': 'Load core and content',
      'qs-diag': 'Diagnostic CLI',
      'qs-diag-1': 'Validate a disc package',
      'qs-diag-2': 'Instruction-level trace',
      'qs-diag-3': 'Headless frame capture',
      'footer-desc': 'A Mattel HyperScan emulator written in Rust · SPG290 low-level emulation',
      'footer-project': 'Project',
      'footer-docs': 'Docs',
      'footer-arch': 'Architecture',
      'footer-hw': 'Hardware',
      'footer-compat': 'Compatibility',
      'footer-contributing': 'Contributing',
      'footer-platforms': 'Platforms',
      'footer-copy': 'BSD 3-Clause License © 2025–2026 Aloys. Built with Rust.'
    }
  };

  // ================================================================
  // i18n — Apply translations
  // ================================================================
  var currentLang = 'zh';

  function applyLang(lang) {
    currentLang = lang;
    var dict = I18N[lang] || I18N.zh;

    document.querySelectorAll('[data-i18n]').forEach(function (el) {
      var key = el.getAttribute('data-i18n');
      if (!dict[key]) return;
      // Prefer innerHTML so intentional <br>/<strong> survive.
      if (dict[key].indexOf('<') !== -1 || el.innerHTML.indexOf('<') !== -1) {
        el.innerHTML = dict[key];
      } else {
        el.textContent = dict[key];
      }
    });

    document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';

    var langBtn = document.getElementById('lang-toggle');
    if (langBtn) langBtn.textContent = lang === 'zh' ? 'EN' : '中';

    buildGallery();
  }

  // ================================================================
  // Gallery
  // ================================================================
  function buildGallery() {
    var root = document.getElementById('gallery-dynamic');
    if (!root) return;

    var html = '<div class="gallery-grid">';
    GAMES.forEach(function (game) {
      var title = currentLang === 'zh' ? game.zh : game.en;
      var note = currentLang === 'zh' ? game.noteZh : game.noteEn;
      var statusKey = game.status === 'pass' ? 'status-pass' : 'status-partial';
      var statusText = (I18N[currentLang] || I18N.zh)[statusKey];
      var badgeClass = game.status === 'pass' ? 'badge-pass' : 'badge-partial';

      html +=
        '<figure class="gallery-item">' +
        '<img loading="lazy" src="' + game.img + '" alt="' + title.replace(/"/g, '&quot;') + '">' +
        '<figcaption class="gallery-meta">' +
        '<h4>' + game.en + '</h4>' +
        '<span class="zh">' + game.zh + ' · ' + note + '</span>' +
        '<span class="badge ' + badgeClass + '">' + statusText + '</span>' +
        '</figcaption></figure>';
    });
    html += '</div>';
    root.innerHTML = html;
  }

  // ================================================================
  // Navigation
  // ================================================================
  function initNav() {
    var navbar = document.getElementById('navbar');
    var toggle = document.querySelector('.nav-toggle');
    var links = document.querySelector('.nav-links');

    function onScroll() {
      if (!navbar) return;
      if (window.scrollY > 24) navbar.classList.add('scrolled');
      else navbar.classList.remove('scrolled');
    }

    window.addEventListener('scroll', onScroll, { passive: true });
    onScroll();

    if (toggle && links) {
      toggle.addEventListener('click', function () {
        links.classList.toggle('open');
        toggle.classList.toggle('open');
      });

      links.querySelectorAll('a').forEach(function (a) {
        a.addEventListener('click', function () {
          links.classList.remove('open');
          toggle.classList.remove('open');
        });
      });
    }
  }

  // ================================================================
  // Fade-in on scroll
  // ================================================================
  function initFadeIn() {
    var fadeEls = document.querySelectorAll('.fade-in-up');
    if (!('IntersectionObserver' in window)) {
      fadeEls.forEach(function (el) { el.classList.add('visible'); });
      return;
    }

    var observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
          observer.unobserve(entry.target);
        }
      });
    }, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });

    fadeEls.forEach(function (el) { observer.observe(el); });
  }

  // ================================================================
  // Stat counters
  // ================================================================
  function animateCounter(el) {
    var target = parseFloat(el.getAttribute('data-target')) || 0;
    var suffix = el.getAttribute('data-suffix') || '';
    var duration = 1400;
    var start = performance.now();

    function tick(now) {
      var t = Math.min(1, (now - start) / duration);
      var eased = 1 - Math.pow(1 - t, 3);
      var value = Math.round(target * eased);
      el.textContent = value + suffix;
      if (t < 1) requestAnimationFrame(tick);
      else el.textContent = target + suffix;
    }

    requestAnimationFrame(tick);
  }

  function initCounters() {
    var statNumbers = document.querySelectorAll('.stat-number[data-target]');
    if (!statNumbers.length) return;

    if (!('IntersectionObserver' in window)) {
      statNumbers.forEach(animateCounter);
      return;
    }

    var statObserver = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          animateCounter(entry.target);
          statObserver.unobserve(entry.target);
        }
      });
    }, { threshold: 0.4 });

    statNumbers.forEach(function (el) { statObserver.observe(el); });
  }

  // ================================================================
  // Hero scan-canvas — soft data-stream / scan field
  // ================================================================
  function initCanvas() {
    var canvas = document.getElementById('scan-canvas');
    if (!canvas) return;

    var ctx = canvas.getContext('2d');
    if (!ctx) return;

    var dpr = Math.min(window.devicePixelRatio || 1, 2);
    var particles = [];
    var raf = 0;

    function resize() {
      var rect = canvas.parentElement.getBoundingClientRect();
      canvas.width = Math.floor(rect.width * dpr);
      canvas.height = Math.floor(rect.height * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      seed(rect.width, rect.height);
    }

    function seed(w, h) {
      particles = [];
      var count = Math.floor((w * h) / 18000);
      for (var i = 0; i < count; i++) {
        particles.push({
          x: Math.random() * w,
          y: Math.random() * h,
          vx: (Math.random() - 0.5) * 0.35,
          vy: 0.25 + Math.random() * 0.55,
          r: 0.8 + Math.random() * 1.6,
          a: 0.12 + Math.random() * 0.35
        });
      }
    }

    function draw() {
      var w = canvas.clientWidth;
      var h = canvas.clientHeight;

      // Gradient wash
      var g = ctx.createLinearGradient(0, 0, w, h);
      g.addColorStop(0, 'rgba(232, 85, 58, 0.10)');
      g.addColorStop(0.45, 'rgba(10, 14, 20, 0.0)');
      g.addColorStop(1, 'rgba(79, 209, 197, 0.12)');
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);

      // Soft grid
      ctx.strokeStyle = 'rgba(232, 85, 58, 0.05)';
      ctx.lineWidth = 1;
      var step = 48;
      for (var x = 0; x < w; x += step) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }
      for (var y = 0; y < h; y += step) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }

      // Drifting particles
      particles.forEach(function (p) {
        p.x += p.vx;
        p.y += p.vy;
        if (p.y > h + 4) { p.y = -4; p.x = Math.random() * w; }
        if (p.x < -4) p.x = w + 4;
        if (p.x > w + 4) p.x = -4;

        ctx.beginPath();
        ctx.fillStyle = 'rgba(232, 85, 58, ' + p.a + ')';
        ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
        ctx.fill();
      });

      raf = requestAnimationFrame(draw);
    }

    resize();
    draw();
    window.addEventListener('resize', resize);

    // Pause when off-screen
    if ('IntersectionObserver' in window) {
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            if (!raf) raf = requestAnimationFrame(draw);
          } else {
            cancelAnimationFrame(raf);
            raf = 0;
          }
        });
      });
      io.observe(canvas);
    }
  }

  // ================================================================
  // Smooth anchors
  // ================================================================
  function initSmoothScroll() {
    document.querySelectorAll('a[href^="#"]').forEach(function (anchor) {
      anchor.addEventListener('click', function (e) {
        var id = anchor.getAttribute('href');
        if (!id || id === '#') return;
        var target = document.querySelector(id);
        if (!target) return;
        e.preventDefault();
        target.scrollIntoView({ behavior: 'smooth', block: 'start' });
      });
    });
  }

  // ================================================================
  // Language toggle
  // ================================================================
  function initLangToggle() {
    var langBtn = document.getElementById('lang-toggle');
    if (!langBtn) return;
    langBtn.addEventListener('click', function () {
      applyLang(currentLang === 'zh' ? 'en' : 'zh');
    });
  }

  // ================================================================
  // Boot
  // ================================================================
  document.addEventListener('DOMContentLoaded', function () {
    applyLang('zh');
    initNav();
    initFadeIn();
    initCounters();
    initCanvas();
    initSmoothScroll();
    initLangToggle();
  });
})();
