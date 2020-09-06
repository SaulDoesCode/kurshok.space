const app = domlib.emitter()
{
    const jsonHTTPMethod = method => (url, body) => fetch(url, {
        method,
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(body)
    })
    const jsonPost = jsonHTTPMethod('POST')
    const jsonPut = jsonHTTPMethod('PUT')

    app.writQuery = async (query = {}) => {
        if (isNaN(query.page)) query.page = 1
        if (!query.kind) query.kind = 'post'
        const res = await jsonPost('/writs', query)
        return await res.json()
    }
}