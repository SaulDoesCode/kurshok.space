import app from '/js/site.min.js'

app.remoteHttp = async (url, req = {}) => {
  if (req.method == null) req.method = "get"
  req.url = url

  return await (await fetch('/remote-http', {
    method: "POST",
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(req),
  })).json()
}
