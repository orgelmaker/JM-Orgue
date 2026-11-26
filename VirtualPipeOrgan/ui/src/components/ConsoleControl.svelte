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
  <button type="button" class="console-control {element.appearance}" class:active class:editing class:selected class:missing
    class:reed={target?.group === 'tong'} class:coupler={element.kind === 'coupler'}
    disabled={!editing && (missing || inert)} aria-label={description} aria-pressed={editing ? selected : active}
    use:playLearn={!editing && !inert && !missing}
    title={missing ? $t('console_builder.missing_control') : `${label} ${pitch}`.trim()}
    on:click={() => dispatch('activate')} on:pointerdown={(event) => dispatch('pointerdown', event)}
    on:keydown={(event) => dispatch('keydown', event)}>
    <span class="control-mount" aria-hidden="true"></span>
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
  .reed .control-name{color:var(--console-reed,#812319)}
  .drawknob.reed .control-name{color:var(--console-draw-reed,#ffb2a2)}
  .rocker-ivory.reed .control-name,.drawknob-ivory.reed .control-name,.tablet.reed .control-name{color:#812319}
  .rocker-wood.reed .control-name,.piston.reed .control-name{color:#ffcdc0}
  .piston.reed.active .control-name{color:#6f2117}
  .coupler .control-name{font-size:clamp(calc(11px / var(--console-scale,1)),.9em,calc(15px / var(--console-scale,1)))}
  .console-control.missing{opacity:.58;filter:saturate(.15)}
  .console-control:disabled{cursor:default}
  .missing-mark{position:absolute;right:2px;top:0;background:#a42b21;color:white;border-radius:50%;width:20px;height:20px;font-weight:600;text-align:center;line-height:20px}
  .console-label{display:flex;align-items:center;justify-content:center;width:100%;height:100%;padding:0 8px;background:transparent;border:none;border-radius:3px;color:var(--console-label,#f4dfb9);text-shadow:0 1px 2px #0008;text-align:center;white-space:pre-line;overflow-wrap:anywhere;font:600 18px/1.1 Georgia,serif;font-size:max(18px,calc(12px / var(--console-scale,1)));letter-spacing:.05em;user-select:none;overflow:hidden}
  button.console-label{cursor:grab;touch-action:none}
  @media(prefers-reduced-motion:reduce){.control-face{transition:none}}
</style>
