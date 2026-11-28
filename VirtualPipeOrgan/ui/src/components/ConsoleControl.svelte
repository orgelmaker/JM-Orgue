<script>
  import { createEventDispatcher } from 'svelte';
  import { t } from '../lib/i18n.js';
  import { midiLearn } from '../lib/midiLearn.js';
  import { registerRegel } from '../lib/registerRegel.js';
  export let element;
  export let target = null;
  export let editing = false;
  export let selected = false;
  export let inert = false;
  const dispatch = createEventDispatcher();
  // Treden: de vorm is een SVG (pianopedaal). Elk exemplaar heeft eigen verloop-ID's nodig.
  $: trede = /^(trede|spoon)-/.test(element.appearance || '');
  $: tredeHout = /-wood$/.test(element.appearance || '');
  $: tw = element.appearance === 'trede-nickel' ? 1.35 : 1;
  $: tredePad = `M${-7 * tw},0 L${7 * tw},0 L${8 * tw},70 C${9 * tw},92 ${24 * tw},104 ${24 * tw},128 C${24 * tw},150 ${12 * tw},162 0,162 C${-12 * tw},162 ${-24 * tw},150 ${-24 * tw},128 C${-24 * tw},104 ${-9 * tw},92 ${-8 * tw},70 Z`;
  $: uid = String(element.id || '').replace(/[^A-Za-z0-9_-]/g, '_');
  $: missing = element.kind !== 'label' && !target;
  $: active = !editing && !!target?.active;
  $: label = element.label || target?.name || $t('console_builder.missing_control');
  $: pitch = element.label ? '' : target?.kind === 'stop'
    ? (target.source?.regel || registerRegel(target.source, $t('stops.ranks_format')))
    : '';
  $: description = editing ? `${$t('console_builder.select_control')}: ${label}` :
    `${label} ${pitch} — ${missing ? $t('console_builder.missing_control') : $t(active ? 'console_builder.on' : 'console_builder.off')}`;
  // Do not install the gesture in design/preview mode: a long drag must never
  // open MIDI learn or change a binding while arranging a console.
  function playLearn(node, enabled) {
    let action;
    function update(next) {
      action?.destroy();
      action = next ? midiLearn(node, { onTrigger: () => dispatch('learn') }) : null;
    }
    update(enabled);
    return { update, destroy() { action?.destroy(); } };
  }
</script>

{#if element.kind === 'label'}
  {#if editing}
    <button type="button" class="console-label" class:selected aria-label="{$t('console_builder.label_element')}: {element.label}"
      aria-pressed={selected} on:click={() => dispatch('activate')} on:pointerdown={(event) => dispatch('pointerdown', event)}
      on:keydown={(event) => dispatch('keydown', event)}>{element.label}</button>
  {:else}
    <div class="console-label">{element.label}</div>
  {/if}
{:else}
  <button type="button" class="console-control {element.appearance}" class:trede class:active class:editing class:selected class:missing
    class:reed={target?.group === 'tong'} class:coupler={element.kind === 'coupler'}
    disabled={!editing && (missing || inert)} aria-label={description} aria-pressed={editing ? selected : active}
    use:playLearn={!editing && !inert && !missing}
    title={missing ? $t('console_builder.missing_control') : `${label} ${pitch}`.trim()}
    on:click={() => dispatch('activate')} on:pointerdown={(event) => dispatch('pointerdown', event)}
    on:keydown={(event) => dispatch('keydown', event)}>
    <span class="control-mount" aria-hidden="true"></span>
    {#if trede}
      <svg class="trede-svg" viewBox="-50 -8 100 184" preserveAspectRatio="xMidYMin meet" aria-hidden="true">
        <defs>
          <linearGradient id="tm-{uid}" x1="0" x2="1"><stop offset="0" style="stop-color:var(--t-c)"/><stop offset=".32" style="stop-color:var(--t-a)"/><stop offset=".62" style="stop-color:var(--t-b)"/><stop offset="1" style="stop-color:var(--t-c)"/></linearGradient>
          <linearGradient id="th-{uid}" x1="0" x2="1"><stop offset="0" stop-color="#3a1e0c"/><stop offset=".35" stop-color="#b9834f"/><stop offset="1" stop-color="#6f4222"/></linearGradient>
          <filter id="ts-{uid}" x="-60%" y="-20%" width="220%" height="150%"><feGaussianBlur stdDeviation="5"/></filter>
        </defs>
        <path d="M{-11 * tw},-6 h{22 * tw} v40 h20 v16 h{-20 - 22 * tw} Z" fill="#0b0705" stroke="#00000088"/>
        <path class="trede-schaduw" d={tredePad} fill="#000" filter="url(#ts-{uid})"/>
        <g class="trede-arm">
          <path d={tredePad} fill="url(#tm-{uid})" stroke="#0008"/>
          <path d="M{-3 * tw},6 L{-3.5 * tw},70 C{-4 * tw},92 {-17 * tw},106 {-17 * tw},128" fill="none" stroke="#fff" stroke-opacity=".5" stroke-width="2.2" stroke-linecap="round"/>
          {#if tredeHout}
            <path d="M-20,112 C-20,100 -12,96 0,96 C12,96 20,100 20,112 L20,132 C20,150 11,157 0,157 C-11,157 -20,150 -20,132 Z" fill="url(#th-{uid})" stroke="#2a1408"/>
            <path d="M-13,108 v34 M-5,103 v50 M4,103 v50 M12,108 v34" stroke="#2a140855"/>
          {/if}
        </g>
      </svg>
    {/if}
    <span class="control-face"><span class="control-name">{label}</span>{#if pitch}<span class="control-pitch">{pitch}</span>{/if}</span>
    <span class="control-light" aria-hidden="true"></span>
    {#if missing}<span class="missing-mark" aria-hidden="true">!</span>{/if}
  </button>
{/if}

<style>
  .console-control{position:relative;width:100%;height:100%;min-width:0;padding:6px;border:1px solid var(--console-case-edge,#17100b);color:var(--console-ink,#251b12);background:var(--console-case,#302017);box-shadow:0 4px 7px #0008,inset 0 1px 1px #ffffff28;border-radius:8px;isolation:isolate;font:inherit;cursor:pointer;touch-action:manipulation;user-select:none;-webkit-user-select:none}
  .console-control:focus-visible,.console-label:focus-visible{outline:3px solid var(--primary,#b8960b);outline-offset:4px}
  .console-control.selected,.console-label.selected{outline:3px solid var(--console-select,#ffc968);outline-offset:4px}
  .console-control.editing{cursor:grab;touch-action:none}
  .console-control.editing:active{cursor:grabbing}
  .control-face{position:absolute;inset:7px 6px 8px;display:flex;flex-direction:column;justify-content:center;align-items:center;gap:2px;padding:3px 5px;border:1px solid var(--console-face-border,#b9a17b);border-radius:4px;background:var(--console-face,#e8d2a7);color:var(--console-ink,#2b2014);box-shadow:inset 0 2px 1px #ffffff8c,inset 0 -5px 7px #5e341632,0 3px 2px #0008;text-shadow:0 1px #ffffff38;overflow:hidden;transition:transform 120ms,box-shadow 120ms;pointer-events:none}
  .control-name{font-size:clamp(calc(11px / var(--console-scale,1)),1em,calc(17px / var(--console-scale,1)));font-weight:600;line-height:1.05;text-align:center;overflow-wrap:anywhere;max-width:100%;white-space:pre-line;display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2;overflow:hidden;flex-shrink:1}
  .control-pitch{font-size:clamp(calc(11px / var(--console-scale,1)),.85em,calc(15px / var(--console-scale,1)));line-height:1;text-align:center;font-weight:500;white-space:nowrap;flex-shrink:0;max-width:100%;overflow:hidden;text-overflow:ellipsis}
  .rocker,.rocker-ivory,.rocker-wood{border-radius:5px;background:linear-gradient(#21170f,#4f3725 50%,#221910)}
  .rocker .control-mount,.rocker-ivory .control-mount,.rocker-wood .control-mount{position:absolute;left:0;right:0;top:47%;height:7px;background:#0e0c0a;box-shadow:0 -1px #ffffff18}
  .rocker .control-face,.rocker-ivory .control-face,.rocker-wood .control-face{inset:5px 6px 9px;border-radius:4px 4px 7px 7px;transform:perspective(230px) rotateX(9deg) translateY(-2px);transform-origin:50% 50%;background:linear-gradient(100deg,#ffffff15,transparent 26%,#4927000d 28%,transparent 29%,#ffffff22 61%,#47290013 66%,transparent 68%),linear-gradient(var(--console-face-top,#f3deb5),var(--console-face,#d8b87e));border-bottom-width:6px;box-shadow:inset 0 2px 2px #fff7,inset 0 -2px 3px #50321533,0 5px 4px #0009}
  .rocker.active .control-face,.rocker-ivory.active .control-face,.rocker-wood.active .control-face{transform:perspective(230px) rotateX(-9deg) translateY(2px);border-top-width:4px;border-bottom-width:2px;box-shadow:inset 0 3px 5px #41240a40,0 1px 2px #0008}
  .rocker-ivory{background:linear-gradient(#1e2427,#545e61 50%,#171e22)}
  .rocker-ivory .control-face{color:#252820;border-color:#bcb8a7;background:linear-gradient(110deg,#fffdf9 4%,#f6f4e9 45%,#e4e2d7 51%,#fffdf6 57%,#dfdcd0 100%);box-shadow:inset 0 3px 2px #fff,inset 0 -2px 2px #a49a7a70,0 5px 4px #0009;text-shadow:0 1px #fff9}
  .rocker-ivory.active .control-face{border-color:#cda85e;background:linear-gradient(110deg,#f5eed7,#fffbed 48%,#eee5ca 52%,#fff9e9);box-shadow:inset 0 3px 5px #73613450,inset 0 -1px #fff,0 1px 2px #0008}
  .rocker-wood{background:linear-gradient(#1c110b,#5a3721 50%,#1a0f09)}
  .rocker-wood .control-face{border-color:#bb8752;background:repeating-linear-gradient(93deg,#eac58e12 0 1px,#3217061a 2px 4px,transparent 5px 10px),linear-gradient(100deg,#9e663a,#7b4727 35%,#a06a3d 70%,#6f3b20);color:#fff0d0;text-shadow:0 -1px 1px #291305;box-shadow:inset 0 2px 2px #edc18a66,inset 0 -3px 3px #35150599,0 5px 4px #000b}
  .rocker-wood.active .control-face{border-color:#ecc684;box-shadow:inset 0 3px 5px #2d120c99,inset 0 -1px #e5b77588,0 1px 2px #0008}
  .control-light{position:absolute;left:calc(50% - 10px);bottom:3px;width:20px;height:3px;border-radius:4px;background:#1b130b;box-shadow:inset 0 1px 1px #000b;pointer-events:none}
  .active .control-light{background:var(--console-light,#ffd36b);box-shadow:0 0 7px var(--console-light,#ffd36b),0 0 2px var(--console-light,#ffd36b)}
  .drawknob,.drawknob-ivory,.piston{padding:0;border:none;background:transparent;box-shadow:none;border-radius:50%}
  .drawknob .control-mount,.drawknob-ivory .control-mount{position:absolute;inset:5px;border-radius:50%;background:radial-gradient(ellipse,#17110d 46%,#725237 48%,#2b1d13 51%,#12100d 58%,#ac835b 60%,#332114 64%,#160e09 70%);box-shadow:0 3px 7px #000a}
  .drawknob .control-face,.drawknob-ivory .control-face{inset:8%;border-radius:50%;border:3px double var(--console-draw-edge,#8f7150);background:radial-gradient(ellipse at 35% 20%,var(--console-draw-highlight,#66503d),var(--console-draw-face,#261e18) 80%);color:var(--console-draw-ink,#f3e5c6);box-shadow:inset 0 2px 2px #ffffff50,inset 0 -3px 3px #000a,0 4px 3px #000b;text-shadow:0 -1px #0009;padding:2px;transform:translateY(2px)}
  .drawknob.active .control-face,.drawknob-ivory.active .control-face{transform:translateY(-4px);box-shadow:inset 0 2px 2px #ffffff50,inset 0 -3px 3px #000a,0 10px 5px #000b;border-color:var(--console-light,#ffd36b)}
  .drawknob .control-light,.drawknob-ivory .control-light{bottom:0;height:4px;width:16px;left:calc(50% - 8px)}
  .drawknob-ivory .control-mount{background:radial-gradient(ellipse,#171611 45%,#b5a986 47%,#544b34 51%,#181915 58%,#d1bf8c 60%,#63533b 64%,#18150e 70%)}
  .drawknob-ivory .control-face{border-color:#ada68a;background:radial-gradient(ellipse at 36% 24%,#fffef6 10%,#f0eddc 58%,#cfc6ab 100%);color:#292a23;box-shadow:inset 0 2px 3px #fff,inset 0 -3px 3px #7c6d4d70,0 4px 3px #000b;text-shadow:0 1px #fff9}
  .drawknob-ivory.active .control-face{border-color:#edc873;box-shadow:inset 0 2px 3px #fff,inset 0 -3px 3px #7c6d4d70,0 10px 5px #000b,0 0 0 2px #9c762a}
  .piston .control-mount{position:absolute;inset:3%;border:1px solid #5a411a;border-radius:50%;background:conic-gradient(from 15deg,#7a5725,#ecd695 12%,#8d692e 25%,#e6cd83 43%,#70501f 58%,#d6b46c 72%,#f4e4ad 82%,#7a5725);box-shadow:inset 0 1px 2px #fff9,inset 0 -2px 3px #24160599,0 3px 5px #000b}
  .piston .control-face{inset:14%;border:1px solid #141614;border-radius:50%;background:radial-gradient(ellipse at 40% 18%,#55594f,#1d221d 74%);color:#fff1ce;text-shadow:0 -1px #0008;box-shadow:inset 0 1px 2px #fff5,inset 0 -2px 3px #0009,0 3px 2px #19160f;transform:translateY(-1px);padding:2px}
  .piston.active .control-face{transform:translateY(3px);background:radial-gradient(ellipse at 40% 18%,#f4d78d,#c39948 78%);color:#231b0c;text-shadow:0 1px #fff6;border-color:#a78036;box-shadow:inset 0 3px 5px #4c350c99,inset 0 -1px #ffeab066,0 0 0 2px #f4d995}
  .piston .control-light{bottom:3%;width:12px;height:3px;left:calc(50% - 6px)}
  .tablet{border-radius:12px 12px 22px 22px;background:linear-gradient(#57412c,#1d1711 80%);border-color:#22180f;box-shadow:inset 0 1px #d7b47a55,0 4px 7px #0009}
  .tablet .control-mount{position:absolute;inset:3px 5px 6px;border-radius:45% 45% 17% 17%/18% 18% 12% 12%;background:linear-gradient(#8f744b,#2a2115);box-shadow:0 2px 2px #0009}
  .tablet .control-face{inset:5px 8px 10px;border-radius:45% 45% 17% 17%/18% 18% 12% 12%;border-color:#c6b692;border-bottom-width:5px;background:linear-gradient(165deg,#fffbeb,#e8dcc0 50%,#f7ebce 72%,#d3bd90);color:#302719;box-shadow:inset 0 3px 4px #fff9,inset 0 -2px 3px #8c6d3f66,0 5px 3px #000a;transform:perspective(220px) rotateX(8deg) translateY(-1px);transform-origin:50% 40%;padding:5px 4px;text-shadow:0 1px #fff9}
  .tablet.active .control-face{transform:perspective(220px) rotateX(-7deg) translateY(3px);border-top-width:3px;border-bottom-width:2px;border-color:#d3ac5c;background:linear-gradient(165deg,#e6d3a6,#fff1cd 55%,#d5bc86);box-shadow:inset 0 3px 5px #80603266,inset 0 -1px #fff8,0 1px 2px #0008}
  .tablet .control-light{bottom:4px}
  .flat{border-radius:7px;background:var(--console-case)}
  .flat .control-face{inset:3px 3px 5px;border-radius:5px;border-width:1px;box-shadow:inset 0 1px #fff6,0 2px 2px #0006;gap:2px}
  .flat.active .control-face{transform:translateY(2px);box-shadow:inset 0 0 0 2px var(--console-light,#ffd36b)}
  /* Extra vormen (0.9.66): ebben- en palissander trekregister met ivoren inleg, ovale porseleinen plaat
     (harmonium/kabinetorgel), tongvormige registertoets, verlichte druktoets en vierkante houten trekker. */
  .drawknob-ebony,.drawknob-rosewood,.oval-porcelain{padding:0;border:none;background:transparent;box-shadow:none;border-radius:50%}
  .drawknob-ebony .control-mount,.drawknob-rosewood .control-mount{position:absolute;inset:3%;border-radius:50%;background:radial-gradient(ellipse at 35% 24%,#544b44,#191512 55%,#060505 100%);box-shadow:0 4px 7px #000b,inset 0 2px 2px #ffffff30,inset 0 -3px 4px #000c;transition:transform 120ms,box-shadow 120ms}
  .drawknob-rosewood .control-mount{background:repeating-conic-gradient(from 20deg,#ffcfa010 0 4deg,#2a0d0518 5deg 9deg),radial-gradient(ellipse at 35% 24%,#b06a42,#6a2f1a 55%,#2d1009 100%)}
  .drawknob-ebony .control-face,.drawknob-rosewood .control-face{inset:13%;border-radius:50%;border:1px solid #8d8468;background:radial-gradient(ellipse at 38% 26%,#fffef6,#ece6d0 65%,#cdc3a6);color:#26251f;box-shadow:inset 0 1px 2px #fff,inset 0 -2px 3px #7c6d4d60,0 0 0 2px #0a0908;text-shadow:0 1px #fff9;padding:2px;transform:translateY(2px)}
  .drawknob-ebony.active .control-mount,.drawknob-rosewood.active .control-mount{transform:translateY(-6px);box-shadow:0 11px 7px #000b,inset 0 2px 2px #ffffff30,inset 0 -3px 4px #000c}
  .drawknob-ebony.active .control-face,.drawknob-rosewood.active .control-face{transform:translateY(-4px);box-shadow:inset 0 1px 2px #fff,inset 0 -2px 3px #7c6d4d60,0 0 0 2px var(--console-light,#ffd36b)}
  .drawknob-ebony .control-name,.drawknob-rosewood .control-name{font-size:clamp(calc(10px / var(--console-scale,1)),.86em,calc(15px / var(--console-scale,1)))}
  .drawknob-ebony .control-light,.drawknob-rosewood .control-light{bottom:0;height:4px;width:16px;left:calc(50% - 8px)}
  .oval-porcelain .control-mount{position:absolute;inset:5% 1%;border-radius:50%;background:linear-gradient(160deg,#e3c98a,#8a6a2e 55%,#5c431a);box-shadow:0 3px 6px #000a,inset 0 1px 1px #fff7}
  .oval-porcelain .control-face{inset:10% 4%;border-radius:50%;border:1px solid #b8b29a;background:radial-gradient(ellipse at 40% 24%,#fffffb,#f2efe2 60%,#d8d2bd);color:#23241f;box-shadow:inset 0 2px 3px #fff,inset 0 -3px 4px #8a7d5a55,0 2px 2px #0008;text-shadow:0 1px #fff9;font-family:Georgia,serif}
  .oval-porcelain.active .control-face{transform:translateY(2px);border-color:#d8b15e;background:radial-gradient(ellipse at 40% 24%,#fffdf0,#f5ecd0 60%,#dccb9d);box-shadow:inset 0 3px 5px #6e5a2a55,0 0 0 2px var(--console-light,#ffd36b)}
  .oval-porcelain .control-light{bottom:0}
  .stopkey{border-radius:6px;background:linear-gradient(#17130f,#3b3129);border-color:#0f0d0b}
  .stopkey .control-mount{position:absolute;left:7%;right:7%;top:3px;height:7px;border-radius:3px;background:#090807;box-shadow:inset 0 1px 2px #000,0 1px #ffffff18}
  .stopkey .control-face{inset:7px 10% 6px;border-radius:5px 5px 46% 46%/5px 5px 28% 28%;border-color:#c9c5b6;background:linear-gradient(175deg,#ffffff,#f3f1e8 55%,#dedacb);color:#22231f;box-shadow:inset 0 2px 2px #fff,inset 0 -4px 5px #8f886f55,0 5px 4px #000a;transform:perspective(240px) rotateX(-9deg);transform-origin:50% 0;text-shadow:0 1px #fff9;padding:3px 4px 10%}
  .stopkey.active .control-face{transform:perspective(240px) rotateX(11deg) translateY(1px);background:linear-gradient(175deg,#fffaf0,#f2e8cd 55%,#d9c9a0);border-color:#d3ac5c;box-shadow:inset 0 3px 5px #6e5a2a55,0 1px 2px #0008}
  .lit{border-radius:9px;background:linear-gradient(#2a2d30,#0f1113);border-color:#07090a}
  .lit .control-face{inset:4px 4px 5px;border-radius:6px;border:1px solid #5d6468;background:linear-gradient(#585d60,#30353a);color:#eceff1;text-shadow:0 -1px #000a;box-shadow:inset 0 1px 1px #ffffff40,inset 0 -3px 5px #0008,0 2px 2px #000a}
  .lit.active .control-face{transform:translateY(1px);background:radial-gradient(ellipse at 50% 38%,#fff8d6,var(--console-light,#ffd36b) 55%,#d49a2e);color:#2a1c05;text-shadow:0 1px #fff8;border-color:#ffe9a8;box-shadow:inset 0 1px 2px #fff,0 0 14px var(--console-light,#ffd36b),0 0 3px #fff8}
  .lit .control-light{display:none}
  .square-wood{border-radius:4px;background:linear-gradient(135deg,#6b4526,#3a2211);border-color:#1e1108;box-shadow:0 4px 7px #000a,inset 0 1px 1px #ffd9a040}
  .square-wood .control-mount{position:absolute;inset:3px;border-radius:3px;background:repeating-linear-gradient(92deg,#f0c08a14 0 1px,#2a130522 2px 4px,transparent 5px 9px),linear-gradient(135deg,#8c5c34,#573318);box-shadow:inset 0 2px 2px #ffdcaa55,inset 0 -3px 4px #1c0c0399}
  .square-wood .control-face{inset:17% 9%;border-radius:2px;border:1px solid #9a8a63;background:linear-gradient(#f4e9cb,#e1d2a6);color:#2b2112;box-shadow:inset 0 0 6px #7a633055,0 1px 1px #0006;font-family:Georgia,serif;transform:translateY(1px)}
  .square-wood.active .control-face{transform:translateY(-3px);border-color:var(--console-light,#ffd36b);box-shadow:inset 0 0 6px #7a633055,0 6px 4px #0009,0 0 0 2px var(--console-light,#ffd36b)}
  .reed .control-name{color:var(--console-reed,#812319)}
  .drawknob.reed .control-name{color:var(--console-draw-reed,#ffb2a2)}
  .rocker-ivory.reed .control-name,.drawknob-ivory.reed .control-name,.tablet.reed .control-name{color:#812319}
  .rocker-wood.reed .control-name,.piston.reed .control-name{color:#ffcdc0}
  .piston.reed.active .control-name{color:#6f2117}
  .drawknob-ebony.reed .control-name,.drawknob-rosewood.reed .control-name,.oval-porcelain.reed .control-name,.stopkey.reed .control-name,.square-wood.reed .control-name{color:#812319}
  .lit.reed .control-name{color:#ffb9a9}
  .lit.reed.active .control-name{color:#7a1f12}
  /* Treden (pianopedaalvorm, Cavaillé-Coll): naamplaatje boven, trede uit een sleuf. Aan = ingetrapt en opzij gehaakt.
     spoon-* is de eerste versie uit 0.9.67 en tekent nu als de overeenkomstige trede. */
  .trede{padding:0;border:none;background:transparent;box-shadow:none;--t-a:#8f959b;--t-b:#4a4f55;--t-c:#15171a}
  .trede-brass,.spoon-brass{--t-a:#fff0b8;--t-b:#c79a3c;--t-c:#5d4010}
  .trede-nickel{--t-a:#f4f6f7;--t-b:#a9b0b6;--t-c:#4d5459}
  .trede .control-mount{display:none}
  .trede .control-face{inset:0 2% auto 2%;height:29%;border-radius:4px;border:1px solid #a9a28a;background:linear-gradient(#fffffb,#ddd6bf);color:#23241f;box-shadow:inset 0 1px 2px #fff,0 2px 3px #0009;text-shadow:0 1px #fff9;padding:1px 3px;font-family:Georgia,serif;gap:0}
  .trede-brass .control-face,.spoon-brass .control-face{border-radius:50%;border:3px solid #b8902f}
  .trede-nickel .control-face{border-color:#6d5215;background:linear-gradient(135deg,#f6e2a0,#c79a3c 50%,#f0d68c);color:#2b1f06;text-shadow:0 1px #fff5}
  .trede .control-name{font-size:clamp(calc(10px / var(--console-scale,1)),.8em,calc(14px / var(--console-scale,1)));font-weight:700}
  .trede-svg{position:absolute;left:0;right:0;top:35%;width:100%;height:65%;overflow:visible;pointer-events:none}
  .trede-arm,.trede-schaduw{transition:transform 140ms,opacity 140ms}
  .trede.active .trede-arm{transform:translate(15px,24px)}
  .trede-schaduw{opacity:.55;transform:translate(9px,13px)}
  .trede.active .trede-schaduw{opacity:.6;transform:translate(19px,30px)}
  .trede .control-light{top:30.5%;bottom:auto;left:calc(50% - 4px);width:8px;height:8px;border-radius:50%}
  .trede.active .control-face{border-color:var(--console-light,#ffd36b)}
  .trede.reed .control-name{color:#812319}
  .coupler .control-name{font-size:clamp(calc(11px / var(--console-scale,1)),.9em,calc(15px / var(--console-scale,1)))}
  .console-control.missing{opacity:.58;filter:saturate(.15)}
  .console-control:disabled{cursor:default}
  .missing-mark{position:absolute;right:2px;top:0;background:#a42b21;color:white;border-radius:50%;width:20px;height:20px;font-weight:600;text-align:center;line-height:20px}
  .console-label{display:flex;align-items:center;justify-content:center;width:100%;height:100%;padding:0 8px;background:transparent;border:none;border-radius:3px;color:var(--console-label,#f4dfb9);text-shadow:0 1px 2px #0008;text-align:center;white-space:pre-line;overflow-wrap:anywhere;font:600 18px/1.1 Georgia,serif;font-size:max(18px,calc(12px / var(--console-scale,1)));letter-spacing:.05em;user-select:none;overflow:hidden}
  button.console-label{cursor:grab;touch-action:none}
  @media(prefers-reduced-motion:reduce){.control-face{transition:none}}
</style>
