🚀 第一部分：产品需求分析 (PRD) & 交互设计
由于用户的需求是“奇奇怪怪”的，核心难点在于： 怎么把模糊的自然语言转化为精确的数据操作，同时让用户感到安全（不敢随便让AI改数据）。

1. 核心功能需求
桌面悬浮态 (Widget Mode): 程序默认是一个极简的“悬浮球”或“动态角色”（类似于桌面宠物）。它安静地呆在屏幕角落，支持鼠标穿透或置顶。

拖拽交互 (Drag & Drop): 用户无需打开文件对话框，直接把Excel文件拖给“悬浮球”，程序立即“吞下”文件并唤醒“工作模式”。

自然语言指令: 用户直接说：“把所有销售额大于1万的标红”、“把上海地区的订单拆分成一个新的表”、“帮我看看哪个季度的增长最慢”。

智能理解 & 追问: 如果用户说“帮我清理数据”，AI应该反问：“具体是指删除空行、去重还是修正格式？”

2. 你可能没想到的“刚需” (补充需求)
数据“快照”与撤销 (Undo/Redo): 处理数据最怕改错。AI每做一步操作前，必须自动备份当前状态。用户可以说“不满意，撤销这一步”。

操作预览 (Diff View): 在AI真正修改文件前，弹出一个对比窗口：“我准备将这50行数据删除，请确认？”（这是准确性的关键保障）。

多表关联上下文: 用户可能拖入两个表，说“把表A的价格更新到表B里”。

可视化图表生成: 用户往往不仅要处理数据，还要看结果。应用应能直接弹窗显示生成的柱状图/折线图。

隐私模式: 提供“脱敏处理”开关，上传给AI API的数据仅包含表头（Schema）和少量样例数据，而不是全量敏感数据。

3. 交互流程设计 (UX Flow)
待机: 桌面右下角一个可爱的动态图标（可以是你的像素风小狗）。

投喂: 用户拖入 data.xlsx -> 图标做出“吞咽”动画 -> 变为“思考/工作”状态的窗口。

对话: 窗口展开，左侧是类似微信的聊天框，右侧是Excel表格的精简预览视图。

指令: 用户输入：“把D列的日期格式统一一下，顺便算出每个月的总和。”

确认: AI回复：“好的，我打算：1. 将D列转为YYYY-MM-DD；2. 创建新表统计月度总和。请确认。” -> 用户点“执行”。

结果: 表格视图实时刷新，用户点击“导出”或直接拖拽保存。

🛠️ 第二部分：技术路线与架构方案 (Rust + Dioxus)
由于要求“怎么准确怎么来”，我强烈建议采用“代码解释器 (Code Interpreter)”模式。即：AI不直接修改Excel文件，而是AI写代码，你的Rust程序负责运行代码。

1. 核心架构图
代码段

graph TD
    User[用户] --> |拖拽Excel/输入指令| DioxusUI[Dioxus 前端 (WebView)]
    DioxusUI --> |IPC通信| RustCore[Rust 后端核心]
    
    subgraph "Rust 后端逻辑"
        RustCore --> |1.读取元数据| PolarsEngine[Polars / Calamine 数据引擎]
        RustCore --> |2.构建Prompt| AI_API[LLM API (GPT/Claude/DeepSeek)]
        AI_API --> |3.返回Python/SQL脚本| ScriptRunner[沙箱执行环境]
        ScriptRunner --> |4.执行数据操作| PolarsEngine
        PolarsEngine --> |5.返回结果预览| DioxusUI
    end
2. 技术选型理由
前端: Dioxus (Desktop)

利用WebView渲染，支持现代CSS/HTML（方便做你喜欢的各种主题、动画）。

Rust原生绑定，与后端逻辑交互极快，打包体积比Electron小得多。

数据处理: Python (PyO3) 或 Polars (Pure Rust)

方案A（高准确性推荐）: 嵌入式Python (PyO3)。AI最擅长写Pandas代码。由于用户需求“奇奇怪怪”，Pandas的灵活性是无敌的。你可以内嵌一个微型Python环境，让LLM生成Pandas代码，你在本地运行。这是最像ChatGPT "Code Interpreter" 的做法。

方案B（高性能推荐）: Polars (Rust)。Polars是Rust写的高性能DataFrame库。你可以让LLM生成 Polars SQL 或 DSL。这需要更精细的Prompt工程，但性能极高且无Python依赖。

决策: 鉴于你要求“准确性”和应对“奇怪需求”，建议方案A（AI生成Python代码 -> Rust调用PyO3执行），因为LLM写Python处理Excel的能力远强于写Rust Polars逻辑。

3. 关键模块实现细节
A. AI 提示词工程 (System Prompt): 你需要设计一个强大的System Prompt，包含：

角色: "你是一个资深数据分析师和Python专家。"

输入: 传入Excel的表头(Header)、前5行数据、列的数据类型。

输出限制: "不要解释，只返回可执行的Python函数，输入是pandas dataframe，输出是处理后的dataframe。"

B. Excel 读写 (Crates):

读取: calamine (速度快，兼容性好)。

写入: rust_xlsxwriter (功能强大，支持格式、图表)。

处理: polars (如果你选择纯Rust路线) 或 pyo3 + pandas (如果你选择嵌入Python)。

C. 窗口管理 (Dioxus/Tao/Wry):

需要处理 window_level 将窗口置顶。

实现“透明窗口”效果，让它看起来像悬浮挂件。

📅 第三部分：实施计划 (Roadmap)
建议分为三个阶段开发，先跑通核心，再优化体验。

第一阶段：原型机 (MVP) —— "能听懂人话的表格工具"
UI搭建: 使用Dioxus搭建一个简单的窗口，左边聊天，右边放一个Table组件。

文件读取: 实现文件拖拽区，使用 calamine 读取Excel并在界面显示前10行。

AI接入: 接入OpenAI/DeepSeek API。

核心逻辑:

用户输入 -> 拼接Prompt (含表头信息) -> 发送给LLM。

LLM返回 -> 解释意图（暂不执行代码，先用文本回复“我建议把A列删除”）。

Rust收到回复 -> 显示在聊天框。

第二阶段：执行者 —— "真正的数据魔术师"
引入执行引擎: 决定使用 PyO3 还是 Polars-SQL。实现代码执行沙箱。

代码生成: 调整Prompt，让LLM返回代码块。

结果渲染: 执行代码后，将新的DataFrame数据转为JSON，推送到前端Dioxus表格中实时更新。

导出功能: 将内存中的数据写入新Excel文件。

第三阶段：完全体 —— "桌面智能伙伴"
悬浮窗模式: 实现窗口的缩小/展开切换，加入你擅长的UI设计（像素风/可爱动画）。

图表支持: 如果LLM建议“画图”，前端使用 ECharts 或 Recharts (React生态在Dioxus中可参考适配) 渲染图表。

复杂功能: 加入撤销/重做栈 (Undo Stack)、多Sheet处理。

💡 给开发者的特别建议
既然你之前在做 GPUI 和 嵌入式，你肯定对性能和底层控制有要求。但对于这个项目，"容错性"比"性能"更重要。 用户的自然语言可能非常模糊（例如：“把那个看起来不对的数删了”），这时候需要在UI上做一个**"交互式清洗"**功能：

AI：“我发现C列有3个数值偏离平均值3倍以上，您是指这些吗？”

[高亮显示这3行]

用户：“对，删掉。”

这个“确认交互”是产品成功的关键。
