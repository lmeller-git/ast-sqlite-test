import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np

# Optional: Force matplotlib to not try to open windows, purely save to file
import matplotlib
matplotlib.use('Agg')

def process_and_visualize(strategy_file, scheduler_file):
    # ==========================================
    # a) Read in DataFrames
    # ==========================================
    df_strat = pd.read_csv(strategy_file)
    df_sched = pd.read_csv(scheduler_file)

    # ==========================================
    # b) Process and Clean Data
    # ==========================================
    df_strat = df_strat.sort_values(by=['tick'])
    df_sched = df_sched.sort_values(by=['tick'])

    df_strat = df_strat.ffill().fillna(0)
    df_sched = df_sched.ffill().fillna(0)

    # ==========================================
    # c) Strategy Stats (Line plots with filling)
    # ==========================================
    metrics_to_plot = ['probability', 'rating', 'cov_increase']
    fig, axes = plt.subplots(len(metrics_to_plot), 1, figsize=(12, 10), sharex=True)

    unique_strategies = df_strat['name'].unique()
    colors = sns.color_palette('husl', len(unique_strategies))

    for ax, metric in zip(axes, metrics_to_plot):
        for idx, strategy in enumerate(unique_strategies):
            strat_data = df_strat[df_strat['name'] == strategy]

            ax.plot(strat_data['tick'], strat_data[metric], label=strategy,
                    color=colors[idx], linewidth=2)

            ax.fill_between(strat_data['tick'], strat_data[metric], 0,
                            color=colors[idx], alpha=0.15)

        ax.set_ylabel(metric.capitalize().replace('_', ' '))
        ax.grid(True, linestyle='--', alpha=0.6)

        if ax == axes[0]:
            # Push legend outside
            ax.legend(title="Strategies", loc="upper left", bbox_to_anchor=(1.02, 1))

    axes[-1].set_xlabel('Tick (Time)')
    fig.suptitle('Strategy Statistics Over Time', fontsize=16)

    # Replaced tight_layout with bbox_inches='tight' in the savefig call
    plt.savefig('strategy_development.png', dpi=300, bbox_inches='tight')
    plt.close() # Clean up memory instead of plt.show()

    # ==========================================
    # d) Scheduler Stats (Growing Corpus Heatmap)
    # ==========================================
    plt.figure(figsize=(14, 10))

    heatmap_data = df_sched.pivot_table(index='name', columns='tick', values='rating')
    heatmap_data = heatmap_data.ffill(axis=1).fillna(0)

    first_appearance = df_sched.groupby('name')['tick'].min().sort_values()
    heatmap_data = heatmap_data.loc[first_appearance.index]

    sns.heatmap(heatmap_data,
                cmap='viridis',
                cbar_kws={'label': 'Rating'},
                yticklabels=False)

    plt.title('Corpus Entry Ratings Over Time (Sorted by Introduction Tick)', fontsize=16)
    plt.xlabel('Tick')
    plt.ylabel('Entries (Oldest at top, Newest at bottom)')

    # Replaced tight_layout with bbox_inches='tight' in the savefig call
    plt.savefig('scheduler_corpus_heatmap.png', dpi=300, bbox_inches='tight')
    plt.close() # Clean up memory instead of plt.show()

# Run the function
# process_and_visualize('strategy_stats.csv', 'scheduler_stats.csv')

if __name__ == "__main__":
    process_and_visualize(
        "../../docker_out/perf_out/strategy_stats.csv",
        "../../docker_out/perf_out/scheduler_stats.csv",
    )
