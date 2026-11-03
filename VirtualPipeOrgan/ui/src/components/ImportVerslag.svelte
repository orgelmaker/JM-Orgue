<script>
  // Importverslag (werkpakket A): wat de orgeldefinitie beloofde, wat er laadt
  // en waarom het verschil er is. Twee standen:
  //   voor    — vóór het laden, als een essentiële klankafhankelijkheid ontbreekt;
  //             het huidige orgel blijft actief tot de gebruiker kiest.
  //   achteraf — het verslag van de laatste import, opnieuw te openen.
  import { createEventDispatcher } from 'svelte';
  import { t } from '../lib/i18n.js';

  export let verslag;
  export let voor = false;
  export let fout = '';
  export let bezig = false;

  const dispatch = createEventDispatcher();

  $: ontbrekend = (verslag?.pakketten ?? []).filter((p) => !p.gevonden);
  $: aanwezig = (verslag?.pakketten ?? []).filter((p) => p.gevonden);
  $: nietSpeelbaar = !!verslag?.niet_speelbaar;
  $: titel = voor
    ? (nietSpeelbaar ? $t('import_report.title_unplayable') : $t('import_report.title_incomplete'))
    : $t('import_report.title_report');
  $: intro = voor
    ? (nietSpeelbaar
        ? $t('import_report.intro_unplayable')
        : $t('import_report.intro_incomplete')
            .replace('{loaded}', verslag?.geladen?.stops ?? 0)
            .replace('{total}', verslag?.verwacht?.stops ?? 0))
    : $t('import_report.intro_report')
        .replace('{loaded}', verslag?.geladen?.stops ?? 0)
        .replace('{total}', verslag?.verwacht?.stops ?? 0);

  function rolTekst(rollen) {
    return (rollen ?? []).map((r) => $t('import_report.role_' + r)).join(', ');
  }
  function pakketNaam(p) {
    return p.naam ? `${p.id} (${p.naam})` : String(p.id);
  }
  function sluit() {
    dispatch('close');
  }
</script>

<svelte:window on:keydown={(e) => { if (e.key === 'Escape') sluit(); }} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="fbk-overlay" on:click|self={sluit}>
  <div class="fbk-modal import-verslag" role="dialog" aria-modal="true" aria-label={titel}>
    <h3>{titel}</h3>
    <p class="iv-intro">{intro}</p>

    <dl class="iv-feiten">
      <dt>{$t('import_report.format')}</dt>
      <dd>{verslag.formaat}{verslag.versie ? ' ' + verslag.versie : ''}</dd>
      <dt>{$t('import_report.stops')}</dt>
      <dd>{verslag.geladen.stops} / {verslag.verwacht.stops}</dd>
      {#if verslag.verwacht.koppels}
        <dt>{$t('import_report.couplers')}</dt>
        <dd>{verslag.geladen.koppels} / {verslag.verwacht.koppels}</dd>
      {/if}
      {#if verslag.verwacht.divisies}
        <dt>{$t('import_report.divisions')}</dt>
        <dd>{verslag.geladen.divisies} / {verslag.verwacht.divisies}</dd>
      {/if}
      {#if verslag.geladen.pijpen}
        <dt>{$t('import_report.pipes')}</dt>
        <dd>{verslag.geladen.pijpen}</dd>
      {/if}
    </dl>

    {#if ontbrekend.length || aanwezig.length}
      <h4>{$t('import_report.packages')}</h4>
      <ul class="iv-lijst">
        {#each ontbrekend as p}
          <li class={(p.rollen ?? []).includes('samples') ? 'iv-ontbreekt' : 'iv-info'}>
            <strong>{pakketNaam(p)}</strong> — {$t('import_report.pkg_missing')}
            <span class="iv-rol">({rolTekst(p.rollen)})</span>
          </li>
        {/each}
        {#each aanwezig as p}
          <li class="iv-ok">
            <strong>{pakketNaam(p)}</strong> — {$t('import_report.pkg_found')}
            <span class="iv-rol">({rolTekst(p.rollen)})</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if verslag.pakketroot}
      <p class="iv-klein">
        {verslag.pakketroot_handmatig ? $t('import_report.root_manual') : $t('import_report.root_found')}:
        <code>{verslag.pakketroot}</code>
      </p>
    {/if}

    {#if verslag.beschermd > 0}
      <p class="iv-fout">{$t('import_report.protected').replace('{n}', verslag.beschermd)}</p>
    {/if}

    {#if verslag.overgeslagen.length}
      <h4>{$t('import_report.skipped').replace('{n}', verslag.overgeslagen.length)}</h4>
      <ul class="iv-lijst iv-scroll">
        {#each verslag.overgeslagen as o}
          <li><strong>{o.naam}</strong> — {o.reden}</li>
        {/each}
      </ul>
    {/if}

    {#if verslag.meldingen.length}
      <h4>{$t('import_report.messages')}</h4>
      <ul class="iv-lijst iv-scroll">
        {#each verslag.meldingen as m}
          <li class={'iv-' + m.ernst}>
            <span class="iv-badge">{$t('import_report.sev_' + m.ernst)}</span>
            <strong>{m.object}</strong>: {m.tekst}
          </li>
        {/each}
      </ul>
    {/if}

    {#if fout}
      <p class="iv-fout">{$t('import_report.folder_error').replace('{error}', fout)}</p>
    {/if}

    <div class="iv-knoppen">
      {#if voor || ontbrekend.length}
        <button class="btn btn-sm" disabled={bezig} on:click={() => dispatch('kiesmap')}>
          {$t('import_report.choose_folder')}
        </button>
        {#if verslag.pakketroot_handmatig}
          <button class="btn btn-ghost btn-sm" disabled={bezig} on:click={() => dispatch('wismap')}>
            {$t('import_report.clear_folder')}
          </button>
        {/if}
      {/if}
      <span class="iv-vul"></span>
      {#if voor}
        {#if !nietSpeelbaar}
          <button class="btn btn-sm" disabled={bezig} on:click={() => dispatch('gedeeltelijk')}>
            {$t('import_report.load_partial')}
          </button>
        {/if}
        <button class="btn btn-primary btn-sm" on:click={sluit}>{$t('import_report.cancel')}</button>
      {:else}
        <button class="btn btn-primary btn-sm" on:click={sluit}>{$t('actions.close')}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .import-verslag { max-width: 44rem; max-height: 85vh; overflow-y: auto; }
  .iv-intro { margin: 0.2rem 0 0.6rem; }
  .iv-feiten { display: grid; grid-template-columns: max-content 1fr; gap: 0.15rem 0.9rem; margin: 0 0 0.6rem; }
  .iv-feiten dt { opacity: 0.7; }
  .iv-feiten dd { margin: 0; }
  h4 { margin: 0.7rem 0 0.25rem; font-size: 0.9rem; }
  .iv-lijst { margin: 0; padding-left: 1.1rem; font-size: 0.85rem; }
  .iv-lijst li { margin: 0.1rem 0; }
  .iv-scroll { max-height: 11rem; overflow-y: auto; }
  /* Leesbaar op de lichte achtergrond van de app. */
  .iv-ontbreekt { color: var(--error, #c44030); }
  .iv-fout { color: var(--error, #c44030); }
  .iv-waarschuwing { color: #8a5200; }
  .iv-info { color: var(--text-secondary, #5c4030); }
  .iv-rol { opacity: 0.7; }
  .iv-klein { font-size: 0.8rem; opacity: 0.8; margin: 0.5rem 0 0; word-break: break-all; }
  .iv-badge { font-size: 0.7rem; text-transform: uppercase; opacity: 0.8; margin-right: 0.3rem; }
  .iv-knoppen { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.9rem; flex-wrap: wrap; }
  .iv-vul { flex: 1; }
</style>
