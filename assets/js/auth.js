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
    app.toast.msg(`auth went through: ` + data.status)
    showAuthMsg(`auth went through: ` + data.status)

    try {
      await app.try_auth_verify()
    } catch(e) {}

    return true
  } else {
    app.toast.error(`auth failed: ` + data.status)
    showAuthMsg(`auth failed: ` + data.status, 5000)
    throw new Error('authentication failed: ' + data.status)
  }
}

app.try_auth_verify = () => new Promise((resolve, reject) => {
  let keepTrying = true
  let to = setTimeout(() => {
    keepTrying = false
    clearInterval(ti)
    reject()
  }, 600000)
  let ti = setInterval(async () => {
    if (!keepTrying) {
      clearInterval(ti)
      clearTimeout(to)
      reject()
      return
    }
    const res = await (await fetch('/auth/verification')).json();
    if (res.ok) {
      app.toast.msg("Auth: " + res.status)
      clearInterval(ti)
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
      }
    }
  }, 1500)
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