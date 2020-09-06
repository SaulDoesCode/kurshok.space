const app = domlib.emitter()

app.writs = []

const jsonPost = app.jsonPost = (url, body) => fetch(url, {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json'
  },
  body: JSON.stringify(body)
})
const jsonPut = app.jsonPut = (url, body) => fetch(url, {
  method: 'PUT',
  headers: {
    'Content-Type': 'application/json'
  },
  body: JSON.stringify(body)
})

app.writQuery = (query = {}) => new Promise((resolve, reject) => {
  if (isNaN(query.page)) query.page = 1
  if (!query.kind) query.kind = 'post'
  jsonPost('/writs', query).then(res => res.json().then(resolve))
})

app.pushWrit = (title, raw_content, tags = ['musings'], opts = {}) => new Promise((resolve, reject) => {
  const raw_writ = {
    title,
    raw_content: raw_content.trim(),
    tags,
    kind: 'post',
    public: true,
    viewable_by: []
  }
  Object.assign(raw_writ, opts)
  jsonPut('/writ', raw_writ)
    .then(res => res.json())
    .then(data => data.ok ? resolve(data.data) : reject(data))
})

app.wq = (page = 1) => fetch('/writs', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json'
  },
  body: `{"page":${page},"kind":"post"}`
})

const pw = () => app.pushWrit(
  'Second Writ',
`
** ain't this some shiiit.. **    
first writ of many that came before
`.trim(),
['testing', 'development']
)

const putComment = ({
  parent_id = 'post:0:0',
  writ_id = 'post:0:0',
  raw_content = 'This is a comment, and it\'s even unique nogal, see: ' + Math.round(Math.random() * 100) + Math.round(Math.random() * 100) + '.',
  author_only = false
} = {}) => new Promise((resolve, reject) => {
  jsonPut('/comment', {
    parent_id,
    writ_id,
    raw_content,
    author_only
  }).then(res => {
    if (res.ok) {res.json().then(resolve).catch(e => {
      reject(new Error("no good, the json was invalid"))
    })}
  }).catch(e => {
    reject(new Error("no good, it didn't work"))
  })
})

const getComments = async (ops = {}, repeat) => {
  if (ops.path == null) ops.path = 'post:0:0/'
  if (ops.page == null) ops.page = 1
  const body = JSON.stringify(ops)
  if (typeof repeat === 'number') {
    for (let i = repeat; i-- > 0;) {
      await fetch('/comments', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body
      })
    }
    return
  }
  return await fetch('/comments', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body
  })
}

const commentRecursively = async (levels, parent_id, writ_id = parent_id) => {
  if (levels-- == 0) return true
  return commentRecursively(levels, (await putComment({ parent_id, writ_id })).data.id, writ_id)
}

const defSetup = async (cWidth = 10, reps) => {
  const writ = await pw()
  const proms = []
  while (cWidth-- > 0) proms.push(commentRecursively(9, writ.id, writ.id))
  await Promise.all(proms)
  await getComments({ path: writ.id + '/' }, reps)
}
