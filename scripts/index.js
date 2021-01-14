const child_process = require('child_process')
const path = require('path')
const chokidar = require('chokidar')
const https = require('https')
const fs = require('fs')


const assetWatcher = chokidar.watch('./assets/', {
    cwd: '../',
    ignored: '*.min.*',
    depth: 20
})

assetWatcher.on('change', filePath => {
    console.log('change', filePath)
    if (filePath.includes('.min')) return
    if (!filePath.includes('assets')) return

    if (filePath.includes('.js')) {
        const {name, dir} = path.parse(filePath)
        
        const cmd = "./node_modules/.bin/terser ../" + filePath + " -c -m --source-map \"root='https://kurshok.space/js/',url='" + name + ".min.js.map'\" -o ../" + dir + '/' + name + ".min.js"

        console.log('going in for minification: ' + cmd)

        child_process.exec(cmd, (err, stdout, stderr) => {
            if (err) return console.error(err)
        })
    }
    
    if (filePath.includes('.css')) {
        const {name, dir} = path.parse(filePath)
        
        const cmd = "./node_modules/.bin/csso ../" + filePath + " -o ../" + dir + '/' + name + ".min.css"

        console.log('going in for minification: ' + cmd)

        child_process.exec(cmd, (err, stdout, stderr) => {
            if (err) return console.error(err)
        })
    }
})

const templateReloadKey = fs.readFileSync("../private/template-reload-key.txt", "utf8")

const templateWatcher = chokidar.watch('./templates/', {
    cwd: '../',
    ignored: '*.min.*',
    depth: 20
})

process.env["NODE_TLS_REJECT_UNAUTHORIZED"] = 0

templateWatcher.on('change', filePath => {
    console.log('template change detected: ', filePath)

    // http.get just kept timing out with 408s, no idea why, so bleh, just use curl cuz it works and it's easy
    child_process.exec('curl -k --header "Authorization: ' + templateReloadKey + '" https://127.0.0.1/reload-templates')
})