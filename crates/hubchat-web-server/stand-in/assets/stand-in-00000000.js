// the stand-in page of hubchat-web-server: see ../index.html
// It asks for the two answers the server makes itself, as the real page does.
const show = async (path) => {
  const r = await fetch(path, { cache: 'no-cache' });
  const p = document.createElement('p');
  p.textContent = `${path}: ${r.status} ${await r.text()}`;
  document.body.append(p);
};
await show('config.json');
await show('healthz');
