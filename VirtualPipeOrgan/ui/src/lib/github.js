// GitHub-koppeling: repo-constante, feedback-URL en de update-check.
// De repo is publiek; api.github.com staat CORS toe, dus een gewone fetch
// vanuit de webview volstaat (geen backend-dependency).

export const GITHUB_REPO = 'orgelmaker/JM-Orgue';

export const githubRepoUrl = () => `https://github.com/${GITHUB_REPO}`;
export const githubIssuesUrl = () => `https://github.com/${GITHUB_REPO}/issues`;

// Vergelijk twee "x.y.z"-versies; >0 als a nieuwer is dan b.
export function compareVersions(a, b) {
  const pa = String(a).replace(/^v/, '').split('.').map(n => parseInt(n, 10) || 0);
  const pb = String(b).replace(/^v/, '').split('.').map(n => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d !== 0) return d;
  }
  return 0;
}

// Stil checken op een nieuwere release. Geeft {version, url} terug wanneer er
// een nieuwere versie is, anders null. Faalt geluidloos (geen internet, repo
// nog niet aangemaakt, rate-limit): update-check mag de opstart nooit storen.
export async function checkForUpdate(currentVersion) {
  try {
    if (GITHUB_REPO.startsWith('INVULLEN')) return null; // nog niet gekoppeld
    const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' },
    });
    if (!res.ok) return null;
    const rel = await res.json();
    const latest = String(rel.tag_name || '').replace(/^v/, '');
    if (!latest || compareVersions(latest, currentVersion) <= 0) return null;
    return { version: latest, url: rel.html_url || githubRepoUrl() + '/releases' };
  } catch (e) {
    return null;
  }
}
