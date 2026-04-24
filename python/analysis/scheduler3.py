import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np
import matplotlib

# Force headless mode
matplotlib.use("Agg")


def comprehensive_fuzzer_analysis(strategy_file, scheduler_file):
    # ==========================================
    # 1. Load and Prep Data
    # ==========================================
    df_strat = pd.read_csv(strategy_file).sort_values(by=["tick"]).ffill().fillna(0)
    df_sched = pd.read_csv(scheduler_file).sort_values(by=["tick"]).ffill().fillna(0)

    # FIX: Get the last known state for EACH entity independently.
    # This prevents dropping strategies that didn't log an event on the very last global tick.
    final_strat = df_strat.groupby("name").last().reset_index()
    final_sched = df_sched.groupby("name").last().reset_index()

    # ==========================================
    # 2. Expanded Strategy Development (Time Series)
    # ==========================================
    # Removed 'syntax_err', leaving 5 active metrics
    metrics = ["probability", "rating", "cov_increase", "attempts", "accepted"]

    # Use a 2x3 grid to nicely fit 5 items
    fig, axes = plt.subplots(2, 3, figsize=(22, 12), sharex=True)
    axes = axes.flatten()

    unique_strategies = df_strat["name"].unique()
    colors = sns.color_palette("husl", len(unique_strategies))

    for idx, metric in enumerate(metrics):
        ax = axes[idx]
        for s_idx, strategy in enumerate(unique_strategies):
            strat_data = df_strat[df_strat["name"] == strategy]
            ax.plot(
                strat_data["tick"],
                strat_data[metric],
                label=strategy,
                color=colors[s_idx],
                linewidth=2,
            )
            ax.fill_between(
                strat_data["tick"], strat_data[metric], 0, color=colors[s_idx], alpha=0.1
            )

        ax.set_ylabel(metric.capitalize().replace("_", " "))
        ax.grid(True, linestyle="--", alpha=0.6)
        ax.tick_params(axis="x", rotation=45)

    # Hide the 6th empty subplot
    axes[5].set_visible(False)

    fig.suptitle("Strategy Statistics Over Time", fontsize=20, y=1.02)

    # FIX: Unified Legend at the bottom of the figure, out of the way of the data
    handles, labels = axes[0].get_legend_handles_labels()
    fig.legend(
        handles,
        labels,
        loc="lower center",
        bbox_to_anchor=(0.5, -0.15),
        ncol=2,
        title="Strategies (Long names mapped below)",
        fontsize="small",
    )

    plt.tight_layout()
    plt.savefig("strategy_development_expanded.png", dpi=300, bbox_inches="tight")
    plt.close()

    # ==========================================
    # 3. Strategy Behavior & Correlations
    # ==========================================
    fig, axes = plt.subplots(1, 3, figsize=(22, 6))
    fig.suptitle("Strategy Behavior & Weighting Analysis", fontsize=18)

    # Plot A: Attempts vs Coverage Increase
    sns.scatterplot(
        data=final_strat,
        x="attempts",
        y="cov_increase",
        size="probability",
        sizes=(50, 400),
        alpha=0.7,
        hue="name",
        legend=False,
        ax=axes[0],
    )
    axes[0].set_title("Attempts vs. Coverage (Dot Size = Final Probability)")
    axes[0].set_xlabel("Total Attempts")
    axes[0].set_ylabel("Total Coverage Increase")

    # Plot B: Coverage vs Rating (Replaced syntax error plot)
    sns.scatterplot(
        data=final_strat,
        x="cov_increase",
        y="rating",
        size="attempts",
        sizes=(50, 400),
        alpha=0.7,
        hue="name",
        legend=False,
        ax=axes[1],
        palette="flare",
    )
    axes[1].set_title("Coverage vs. Final Rating (Dot Size = Total Attempts)")
    axes[1].set_xlabel("Total Coverage Increase")
    axes[1].set_ylabel("Final Rating")

    # Plot C: Strategy Correlation Matrix (Removed syntax_err)
    strat_corr_cols = ["attempts", "accepted", "cov_increase", "rating", "probability"]
    corr_matrix_strat = final_strat[strat_corr_cols].corr()
    sns.heatmap(
        corr_matrix_strat, annot=True, cmap="coolwarm", vmin=-1, vmax=1, ax=axes[2], fmt=".2f"
    )
    axes[2].set_title("Strategy Metric Correlations")

    plt.tight_layout()
    plt.savefig("strategy_behavior.png", dpi=300, bbox_inches="tight")
    plt.close()


# Run the analysis
# comprehensive_fuzzer_analysis('strategy_stats.csv', 'scheduler_stats.csv')
if __name__ == "__main__":
    comprehensive_fuzzer_analysis(
        "../../docker_out/perf_out/strategy_stats.csv",
        "../../docker_out/perf_out/scheduler_stats.csv",
    )
