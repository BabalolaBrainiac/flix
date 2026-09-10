import assert from 'node:assert/strict';

export async function checkSearch(page) {
  let finishAnime;
  const animePending = new Promise(resolve => { finishAnime = resolve; });
  let sourceRequests = 0;
  await page.route('**/api/search', async route => {
    const { catalog } = route.request().postDataJSON();
    if (catalog === 'anime') await animePending;
    await route.fulfill({ json: { notes: [], items: catalog === 'series' ? [
      { id: 'tt123', name: 'Anime fixture', media_type: 'series', is_anime: false },
    ] : [] } });
  });
  await page.route('**/api/episodes', route => route.fulfill({ json: {
    is_anime: true, episodes: [{ id: 'tt123:1:1', stream_id: 'tt123:1:1', season: 1, episode: 1, title: 'First episode' }],
  } }));
  await page.route('**/api/streams', route => {
    sourceRequests += 1;
    const request = route.request().postDataJSON();
    assert.equal(request.stream_id, 'tt123:1:1');
    assert.equal(request.is_anime, true);
    return route.fulfill({ json: { is_anime: true, streams: [
      { source_id: 'fixture', name: 'Fixture source', quality: '1080p', is_recommended: true },
    ] } });
  });
  try {
    await page.locator('nav').getByRole('button', { name: 'Search', exact: true }).click();
    await page.getByPlaceholder('Search movies, TV shows, and anime...').fill('fixture');
    await page.locator('form').getByRole('button', { name: 'Search', exact: true }).click();
    const result = page.getByRole('button', { name: /Anime fixture/ });
    await result.waitFor({ timeout: 2000 });
    await page.getByText('Searching Anime… You can open a result now.').waitFor({ timeout: 2000 });
    await result.click();
    await page.getByRole('heading', { name: 'Choose an episode' }).waitFor();
    assert.equal(await page.locator('#selected-sources').count(), 0);
    assert.equal(sourceRequests, 0);
    await page.getByRole('button', { name: /First episode/ }).click();
    await page.getByText('Original audio and English subtitles. Anime currently needs an external player.').waitFor();
    const browserOption = await page.locator('option[value="browser"]').evaluate(option => ({ disabled: option.disabled, html: option.outerHTML }));
    assert.equal(browserOption.disabled, true, browserOption.html);
    assert.equal(sourceRequests, 1);
  } finally {
    finishAnime();
  }
  console.log('PASS: results before a slow provider, episode selection, anime routing, and player limits.');
}
