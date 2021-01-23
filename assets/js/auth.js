import app from '/js/site.min.js'
const d = app.d, df = d.domfn

const authLauncher = d.query('.auth-launcher') || df.div.auth_launcher({$: 'body'}, 'Auth')

const {
  authView,
  usernameInput,
  emailInput,
  authBtn
} = d.h `
<section class="auth-view" ref="authView">
  <div>
    <div class="auth-form-field">
      <label for="auth-username">Username</label>
      <input type="text" name="username" id="auth-username" ref="usernameInput">
      <label for="auth-email">Email</label>
      <input type="text" name="email" id="auth-email" ref="emailInput">
    </div>
    <button class="submit" ref="authBtn">authenticate</button>
  </div>
</section>`.collect()

app.authViewToggle = app.setupToggleSituation(
  authLauncher,
  app.authView = authView,
  'body',
  {background: true}
)

const authMsg = df.div({class: 'auth-msg'})

const showAuthMsg = (msg, time) => {
  df.attrToggle(authBtn, 'hidden', true)
  authMsg.textContent = msg
  if (!authView.contains(authMsg)) {
    authView.append(authMsg)
  }
  if (time) {
    df.remove(authMsg, time).then(() => {
      df.attrToggle(authBtn, 'hidden', undefined)
      authView.append(authBtn)
    })
  }
}

app.authenticate = async (
  username = usernameInput.value.trim(),
  email = emailInput.value.trim()
) => {
  if (username == '') {
    if (email == '' || !email.includes('@')) {
      showAuthMsg('username & email are invalid', 4500)
    } else {
      showAuthMsg('username is invalid', 4500)
    }
    throw new Error('username is invalid')
  }
  if (email == '' || !email.includes('@')) {
    showAuthMsg('email is invalid', 4500)
    throw new Error('email is invalid')
  }

  showAuthMsg('attempting authentication...')


  const res = await app.jsonPost('/auth', { username, email })
  const data = await res.json()

  console.log(data)
  if (data.ok) {
    app.toast.msg(`auth email is sending: ` + data.status)
    showAuthMsg(`auth email is sending: ` + data.status)

    showAuthMsg(`auth email status: ` + await app.check_email_status())
    await app.try_auth_verify()

    return true
  } else {
    app.toast.error(`auth failed: ` + data.status)
    showAuthMsg(`auth failed: ` + data.status, 5000)
    throw new Error('authentication failed: ' + data.status)
  }
}

app.check_email_status = () => new Promise((resolve, reject) => {
  let keepTrying = true
  const to = setTimeout(() => {
    keepTrying = false
    clearInterval(ti)
    reject()
  }, 600000)
  let to2 = setTimeout(async function check() {
    if (!keepTrying) {
      clearTimeout(to)
      clearTimeout(to2)
      reject()
      return
    }
    const res = await (await fetch('/auth/email-status')).json();
    if (res.ok) {
      app.toast.msg("Auth Email: " + res.status)
      clearTimeout(to2)
      clearTimeout(to)
      resolve(res.status)
    } else {
      console.log(res)
      if (
        res.status.includes('failed to send') ||
        res.status.includes('Failed to read preauth token') ||
        res.status.includes('preauth cookie')
      ) {
        clearTimeout(to2)
        clearTimeout(to)
        return reject()
      }
      to2 = setTimeout(check, 1500)
    }
  }, 1500)
})

app.try_auth_verify = () => new Promise((resolve, reject) => {
  let keepTrying = true
  const to = setTimeout(() => {
    keepTrying = false
    clearTimeout(to2)
    reject()
  }, 600000)
  let to2 = setTimeout(async function check() {
    if (!keepTrying) {
      clearTimeout(to2)
      clearTimeout(to)
      reject()
      return
    }
    const res = await (await fetch('/auth/verification')).json();
    if (res.ok) {
      app.toast.msg("Auth: " + res.status)
      clearTimeout(to2)
      clearTimeout(to)
      resolve()
      setTimeout(() => {
        window.location.reload()
      }, 3000)
    } else {
      console.log(res)
      if (res.status.includes('expire') && res.status.includes('preauth')) {
        setTimeout(() => {
          window.location.reload()
        }, window.location.hostname == 'localhost' ? 4500 : 800)
        clearTimeout(to2)
        clearTimeout(to)
        reject()
      }
      to2 = setTimeout(check, 1500)
    }
  }, 3000)
})

const authClickHandle = d.once.click(authBtn, async e => {
  try {
    await app.authenticate()
  } catch(e) {
    console.error(e)
    authClickHandle.on()
  }
})

let mlSuccess = localStorage.getItem('ml-success')
window.addEventListener('storage', () => {
  if (mlSuccess !== localStorage.getItem('ml-success')) {
    localStorage.removeItem('ml-success')
    if (window.location.hostname == 'localhost') {
      document.body.innerHTML = `<h1>Yeah, auth worked and all that.</h1>`
    } else {
      window.close()
    }
  }
})

if (window.location.pathname === '/') {
  app.authViewToggle.toggleView()
}