const app = domlib.emitter()
{
  const {
    titleInput,
    rawContentInput,
    preview,
    tagsInput,
    writKind,
    pushWritButton,
    writList
  } = domlib.html`
<section class="writ-editor">
  <div class="title">
      <label>title</label>
      <input type="text" name="title" ref="titleInput">
  </div>
  <div class="editor">
      <div class="pad">
          <label>editor</label>
          <textarea name="raw_content" cols="30" rows="10" ref="rawContentInput">
          </textarea>
      </div>
      <div class="preview" ref="preview">
      </div>
  </div>
  <div class="tager">
      <label>tags</label>
      <input type="text" name="tags" ref="tagsInput">
  </div>
  <div class="kind">
      <label>kind</label>
      <input type="text" name="kind" value="post" ref="writKind">
  </div>
  <br>
  <button ref="pushWritButton">push writ</button>
</section>

<section class="writ-list" ref="writList"></section>
`.renderCollect()

rawContentInput.value = ''

app.writs = []

app.post = (url, body) => fetch(url, {
  method: 'POST',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify(body)
})
app.put = (url, body) => fetch(url, {
  method: 'PUT',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify(body)
})

app.writQuery = (query = {}) => new Promise((resolve, reject) => {
  if (isNaN(query.page)) query.page = 1
  if (!query.kind) query.kind = 'post'
  app.post('/writs', query).then(res => res.json().then(resolve))
})

app.pushWrit = (rawwrit = {}) => new Promise((resolve, reject) => {
  if (!rawwrit.title) rawwrit.title = titleInput.value.trim()
  if (!rawwrit.raw_content) rawwrit.raw_content = rawContentInput.value.trim()
  if (!rawwrit.tags) rawwrit.tags = tagsInput.value.trim().split(',').map(t => t.trim())
  if (!rawwrit.kind) rawwrit.kind = writKind.value.trim() || 'post'
  if (rawwrit.public == null) rawwrit.public = true
  if (rawwrit.viewable_by == null) rawwrit.viewable_by = []

  for (const [k, v] of Object.entries(rawwrit)) {
    if (k == 'public') continue
    if (k == 'viewable_by') continue
    if (v == null || !v.length) {
      domlib.run(() => {
        throw new Error(`pushWrit: Invalid/Empty field - ${k}`)
      })
      return reject({ok: false})
    }
  }

  app.put('/writ', rawwrit)
    .then(res => res.json())
    .then(data => data.ok ? resolve(data.data) : reject(data))
})

const pushHandle = domlib.once.click(pushWritButton, e => {
  const res = app.pushWrit()
  if (!res.ok) pushHandle.on()
})

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
}
