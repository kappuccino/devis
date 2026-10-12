<script setup lang="ts">
import { ref } from "vue";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Password from "primevue/password";
import Message from "primevue/message";
import logo from "../assets/logo-cahors.png";
import { licence, login } from "../licence";
import { errorMessage } from "../format";

const email = ref("");
const password = ref("");
const busy = ref(false);
const error = ref("");

async function submit() {
  if (!email.value.trim() || !password.value) return;
  busy.value = true;
  error.value = "";
  try {
    await login(email.value, password.value);
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="login">
    <form class="card" @submit.prevent="submit">
      <div class="brand">
        <img :src="logo" alt="Cahors" />
        <span>Devis</span>
      </div>
      <h1>Connexion</h1>
      <p class="muted">
        Connectez-vous avec le compte fourni par l'administrateur de l'appli. Internet est nécessaire pour
        se connecter ; ensuite, l'appli fonctionne aussi hors ligne pendant quelques jours.
      </p>
      <Message v-if="licence.error && !error" severity="warn" :closable="false">{{ licence.error }}</Message>
      <label for="login-email">Email</label>
      <InputText id="login-email" v-model="email" type="email" autocomplete="username" autofocus />
      <label for="login-password">Mot de passe</label>
      <Password
        v-model="password"
        input-id="login-password"
        :feedback="false"
        toggle-mask
        fluid
        :input-props="{ autocomplete: 'current-password' }"
      />
      <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
      <Button type="submit" label="Se connecter" icon="pi pi-sign-in" :loading="busy" :disabled="!email.trim() || !password" />
    </form>
  </div>
</template>

<style scoped>
.login {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--app-sidebar-bg);
  padding: 1rem;
}

.card {
  width: 100%;
  max-width: 380px;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  background: var(--app-surface);
  color: var(--app-text);
  border-radius: 10px;
  padding: 1.75rem;
  box-shadow: 0 10px 40px rgb(0 0 0 / 0.35);
}

.brand {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.6rem 0.8rem;
  border-radius: 8px;
  background: var(--app-sidebar-bg);
  color: var(--app-sidebar-text);
  font-weight: 600;
  align-self: flex-start;
}

.brand img {
  width: 110px;
}

h1 {
  font-size: 1.3rem;
  margin: 0.4rem 0 0;
}

.muted {
  margin: 0 0 0.4rem;
  font-size: 0.9rem;
}

label {
  font-size: 0.85rem;
  font-weight: 600;
  margin-top: 0.3rem;
}

button {
  margin-top: 0.6rem;
}
</style>
