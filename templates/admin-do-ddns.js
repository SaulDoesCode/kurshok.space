{
const DO_TOKEN = `{{ do_token }}`
const DOMAIN = `grimstack.io`

app.fetchRecords = async () => {
    try {
        const res = await app.remoteHttp(`https://api.digitalocean.com/v2/domains/${DOMAIN}/records`, {
            bearer_token: DO_TOKEN,
        })
        if (!res.ok) throw new Error(`fetchRecords error:${res.status}`)
        return JSON.parse(res.data.body).domain_records
    } catch(e) {
        throw new Error(`fetchRecords: bad request, ${e}`)
    }
}

app.updateRecord = async (domain, rType = 'A') => {
    const ipv4 = (await app.remoteHttp('http://ipv4bot.whatismyipaddress.com')).data.body
    console.log(ipv4)

    let record
    (await app.fetchRecords()).forEach(r => {
        if (r.type == rType && r.name == domain) {
            record = r
        }
    });
    if (record == null || record.id == null) {
        throw new Error(`no such record as ${domain} found, can't update`)
    }

    const updateRequest = {name: record.name}

    let res
    const doUpdate = async () => await app.remoteHttp(`https://api.digitalocean.com/v2/domains/${DOMAIN}/records/${record.id}`, {
        method: 'put',
        content_type: 'application/json',
        bearer_token: DO_TOKEN,
        body: JSON.stringify(updateRequest),
    })

    try {
        res = doUpdate()
    } catch(e) {
        res = doUpdate()
    }
    if (res != null) {
        console.log('DigitalOcean API: update record response', res)
    } else {
        console.log('DigitalOcean API: update record failed epically')
    }
}

app.updateHome = async () => {
    return await app.updateRecord('home')
}

}