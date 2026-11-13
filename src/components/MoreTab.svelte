<script lang="ts">
  const roadmap = [
    { name: "Полка для файлов", desc: "Перетаскивание файлов на каплю и обратно", status: "Следующий этап" },
    { name: "Инструменты текста", desc: "Раскладка, регистр, JSON, Base64, QR", status: "Этап 2" },
    { name: "Скриншоты и OCR", desc: "Захват области, пипетка, текст с экрана", status: "Этап 3" },
    { name: "Командная строка", desc: "Быстрый запуск по Alt+Space", status: "Этап 4" },
  ];
</script>

<div class="grid">
  {#each roadmap as item}
    <div class="card">
      <div class="top">
        <span class="name">{item.name}</span>
        <span class="badge mono">{item.status}</span>
      </div>
      <p>{item.desc}</p>
    </div>
  {/each}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 10px;
    height: 100%;
    align-content: start;
  }
  .card {
    background: var(--ink-2);
    border: 1px solid var(--line);
    border-radius: var(--r-m);
    padding: 14px 16px;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }
  .name {
    font-weight: 600;
    font-size: 13.5px;
  }
  .badge {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--ink-3);
    color: rgb(var(--accent-rgb));
  }
  p {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
  }
</style>
