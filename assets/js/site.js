import domlib from '/js/domlib.min.js'

const app = domlib.emitter()
app.d = domlib
const jsonHTTPMethod = method => (url, body) => fetch(url, {
    method,
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify(body)
})
app.jsonPost = jsonHTTPMethod('POST')
app.jsonPut = jsonHTTPMethod('PUT')

app.writQuery = async (query = {}) => {
    if (isNaN(query.page)) query.page = 1
    if (!query.kind) query.kind = 'post'
    const res = await app.jsonPost('/writs', query)
    return await res.json()
}

export default app
